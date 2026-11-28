mod gpu;
mod input;
mod telemetry;
mod trace;
mod window;

use gpu::GpuRenderer;
use input::{InputQueue, RawInputEvent};
use std::env;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use telemetry::{TelemetryPacket, TelemetryServer};
use trace::{FlightRecorder, FrameTimings};
use wasmtime::component::{Component, Linker, ResourceTable};
use wasmtime::{Config, Engine, Store};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiView};
use window::WindowManager;
use winit::event::{ElementState, Event, MouseButton, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::platform::pump_events::{EventLoopExtPumpEvents, PumpStatus};

// Generate component bindings from the WIT contracts in `wit/`.
wasmtime::component::bindgen!({
    path: "../wit",
    world: "groot-runner",
});

struct HostState {
    window_mgr: WindowManager,
    gpu: GpuRenderer,
    inputs: Arc<Mutex<InputQueue>>,
    wasi: WasiCtx,
    table: ResourceTable,
    event_loop: Arc<SendEventLoop>,
    frame_interval: Duration,
    next_frame: Option<Instant>,
    close_requested: bool,
    vsync: bool,
    in_event_loop: bool,

    // --- Telemetry & Flight Recorder (debug mode only) ---
    telemetry: Option<TelemetryServer>,
    recorder: Option<FlightRecorder>,
    frame_index: u64,
    frame_start: Option<Instant>,
    last_guest_end: Option<Instant>,
    frame_abi_us: u32,
    frame_abi_calls: u32,
    last_dt_secs: f32,

    // --- Manifest values (from groot.toml, injected by groot CLI via env vars) ---
    manifest_width:  u32,
    manifest_height: u32,
    manifest_title:  String,
}

struct SendEventLoop(Mutex<Option<EventLoop<()>>>);
unsafe impl Send for SendEventLoop {}
unsafe impl Sync for SendEventLoop {}

impl HostState {
    fn pump_window_events(&mut self) -> bool {
        let mut guard = self.event_loop.0.lock().unwrap();
        let Some(el) = guard.as_mut() else { return true; };
        let mut close_req = false;
        let mut resize_to: Option<(u32, u32)> = None;

        let status = el.pump_events(Some(Duration::ZERO), |event, target| {
            match event {
                Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => {
                    close_req = true;
                    target.exit();
                }
                Event::WindowEvent { event: WindowEvent::Resized(size), .. } => {
                    resize_to = Some((size.width, size.height));
                }
                Event::WindowEvent { event: WindowEvent::CursorMoved { position, .. }, .. } => {
                    let mut q = self.inputs.lock().unwrap();
                    q.set_mouse(position.x as f32, position.y as f32);
                    q.push(RawInputEvent {
                        event_type: 2,
                        key_code: 0,
                        mouse_x: position.x as f32,
                        mouse_y: position.y as f32,
                    });
                }
                Event::WindowEvent { event: WindowEvent::MouseInput { state, button, .. }, .. } => {
                    let kind = if state == ElementState::Pressed { 3 } else { 4 };
                    let is_pressed = state == ElementState::Pressed;
                    let code = match button {
                        MouseButton::Left => 1,
                        MouseButton::Right => 2,
                        MouseButton::Middle => 3,
                        _ => 0,
                    };
                    let mut q = self.inputs.lock().unwrap();
                    q.on_mouse_button(code, is_pressed);
                    let (mx, my) = (q.mouse_x, q.mouse_y);
                    q.push(RawInputEvent {
                        event_type: kind,
                        key_code: code,
                        mouse_x: mx,
                        mouse_y: my,
                    });
                }
                Event::WindowEvent { event: WindowEvent::KeyboardInput { event, .. }, .. } => {
                    let kind = if event.state == ElementState::Pressed { 0 } else { 1 };
                    let is_pressed = event.state == ElementState::Pressed;
                    let code = match event.physical_key {
                        PhysicalKey::Code(code) => key_code_to_u32(code),
                        _ => 0,
                    };
                    let mut q = self.inputs.lock().unwrap();
                    q.on_key(code, is_pressed);
                    let (mx, my) = (q.mouse_x, q.mouse_y);
                    q.push(RawInputEvent {
                        event_type: kind,
                        key_code: code,
                        mouse_x: mx,
                        mouse_y: my,
                    });
                }
                _ => (),
            }
        });

        if let Some((w, h)) = resize_to {
            self.gpu.resize(w, h);
        }
        if close_req || matches!(status, PumpStatus::Exit(_)) {
            self.close_requested = true;
            false
        } else {
            true
        }
    }

    fn present_frame(&mut self, r: f64, g: f64, b: f64) {
        let t_pump_start = Instant::now();
        if !self.in_event_loop {
            let _ = self.pump_window_events();
            if self.close_requested {
                std::process::exit(0);
            }
        }
        let pump_us = t_pump_start.elapsed().as_micros() as u32;

        let now = Instant::now();
        let total_us = self
            .frame_start
            .map(|s| s.elapsed().as_micros() as u32)
            .unwrap_or(16_666);
        self.frame_start = Some(now);

        let dt_secs = (total_us as f32 / 1_000_000.0).clamp(0.0001, 0.1);
        self.last_dt_secs = dt_secs;

        // Frame Pacing:
        // When VSync is enabled (Fifo), the GPU driver presentation queue handles
        // vertical retrace synchronization with hardware precision. Sleeping on
        // top of Fifo causes missed vblanks and severe microstutter.
        // Only sleep if VSync is disabled.
        if !self.vsync {
            if let Some(target) = self.next_frame {
                if target > now {
                    std::thread::sleep(target - now);
                }
                self.next_frame = Some(target + self.frame_interval);
            } else {
                self.next_frame = Some(now + self.frame_interval);
            }
        }

        self.gpu.draw_frame(r, g, b);

        if self.in_event_loop {
            if let Some(win) = self.window_mgr.window() {
                win.request_redraw();
            }
        }

        let guest_plus_abi = self
            .last_guest_end
            .map(|s| s.elapsed().as_micros() as u32)
            .unwrap_or(0);
        let guest_us = guest_plus_abi.saturating_sub(self.frame_abi_us);

        self.frame_index += 1;
        let fps = if total_us > 0 {
            1_000_000.0 / total_us as f32
        } else {
            60.0
        };

        let timings = FrameTimings {
            frame_index: self.frame_index,
            total_us,
            pump_us,
            guest_us,
            abi_us: self.frame_abi_us,
            gpu_upload_us: self.gpu.frame_stats.upload_us,
            gpu_encode_us: self.gpu.frame_stats.encode_us,
            present_wait_us: self.gpu.frame_stats.present_wait_us,
            dt_ms: dt_secs * 1000.0,
            fps,
            abi_calls: self.frame_abi_calls,
            draw_calls: self.gpu.stats_draw_calls,
            instances_3d: self.gpu.stats_instances_3d,
            triangles: self.gpu.stats_triangles_rendered,
            gpu_buffer_reallocs: self.gpu.frame_stats.buffer_reallocs,
            mesh_uploads: self.gpu.frame_stats.mesh_uploads,
            surface_reconfigs: self.gpu.frame_stats.surface_reconfigs,
            arena_peak_bytes: 0,
            arena_reclaimed_bytes: 0,
            arena_promoted_bytes: 0,
            entities_alive: 0,
            active_span_count: 0,
        };

        // Reset per-frame GPU counters that accumulate across calls
        self.gpu.frame_stats.buffer_reallocs = 0;
        self.gpu.frame_stats.mesh_uploads = 0;
        self.gpu.frame_stats.surface_reconfigs = 0;

        let mut is_spike = false;
        let mut spike_count = 0u64;
        let mut target_frame_us = self.frame_interval.as_micros() as u32;

        if let Some(ref mut rec) = self.recorder {
            rec.adapter_name = self.gpu.adapter_name.clone();
            is_spike = rec.record_frame(timings);
            spike_count = rec.spike_count;
            target_frame_us = rec.target_frame_us;

            // Check if TUI requested LLM flight recorder dump
            if let Some(ref telem) = self.telemetry {
                if telem.check_dump_requested() {
                    let dump_path = format!("/tmp/groot-report-{}.txt", std::process::id());
                    if rec.export_llm_report(&dump_path).is_ok() {
                        telem.send_dump_reply(format!("DUMPED:{}\n", dump_path));
                    }
                }
            }
        }

        if let Some(ref telem) = self.telemetry {
            let (mouse_x, mouse_y, mouse_mask, keys_count) = {
                let q = self.inputs.lock().unwrap();
                (q.mouse_x, q.mouse_y, q.mouse_buttons_mask(), q.pressed_keys_count())
            };
            telem.publish(TelemetryPacket {
                frame_index: self.frame_index,
                frame_time_us: total_us,
                fps,
                quads_rendered: self.gpu.vertex_count / 6,
                triangles_rendered: self.gpu.stats_triangles_rendered,
                instances_3d: self.gpu.stats_instances_3d,
                vertex_buffer_bytes: (self.gpu.vertex_count * 8 * 4) + (self.gpu.stats_instances_3d * 20 * 4),
                draw_calls: self.gpu.stats_draw_calls,
                mouse_x,
                mouse_y,
                mouse_buttons_mask: mouse_mask,
                pressed_keys_count: keys_count,
                guest_us,
                abi_us: self.frame_abi_us,
                gpu_upload_us: self.gpu.frame_stats.upload_us,
                gpu_encode_us: self.gpu.frame_stats.encode_us,
                present_wait_us: self.gpu.frame_stats.present_wait_us,
                pump_us,
                abi_calls: self.frame_abi_calls,
                target_frame_us,
                spike_count: spike_count as u32,
                gpu_reallocs: timings.gpu_buffer_reallocs,
                mesh_uploads: timings.mesh_uploads,
                surface_reconfigs: timings.surface_reconfigs,
                is_spike: if is_spike { 1 } else { 0 },
                vsync_enabled: if self.vsync { 1 } else { 0 },
                ..Default::default()
            });
        }

        // Reset per-frame ABI timers & mark end of present for guest timing
        self.frame_abi_us = 0;
        self.frame_abi_calls = 0;
        self.last_guest_end = Some(Instant::now());
    }

    /// Count one ABI call and record its elapsed time (call after the work is done).
    #[inline(always)]
    fn abi_end(&mut self, t: Instant) {
        self.frame_abi_us += t.elapsed().as_micros() as u32;
        self.frame_abi_calls += 1;
    }
}

impl WasiView for HostState {
    fn ctx(&mut self) -> &mut WasiCtx {
        &mut self.wasi
    }

    fn table(&mut self) -> &mut ResourceTable {
        &mut self.table
    }
}

impl ube::graphics::window::Host for HostState {
    fn create_window(&mut self, width: u32, height: u32, title: String) {
        let _t = Instant::now();
        log::info!("creating window {}x{} '{}'", width, height, title);

        let has_el = {
            let guard = self.event_loop.0.lock().unwrap();
            guard.is_some()
        };
        if !has_el {
            log::error!("create-window called after the event loop started");
            self.abi_end(_t);
            return;
        }

        // Re-lock to get the event loop ref for window creation.
        {
            let el_guard = self.event_loop.0.lock().unwrap();
            let el = el_guard.as_ref().unwrap();
            self.window_mgr.create_window(el, width, height, &title, true);
        }
        let vsync = self.vsync;
        if let Some(win) = self.window_mgr.window() {
            self.gpu.init_surface(win, width, height, vsync);
        }
        self.abi_end(_t);
    }
}

impl ube::graphics::input::Host for HostState {
    fn is_key_down(&mut self, code: u32) -> bool {
        let _t = Instant::now();
        let result = self.inputs.lock().unwrap().keys_down.contains(&code);
        self.abi_end(_t);
        result
    }
    fn is_mouse_down(&mut self, button: u32) -> bool {
        let _t = Instant::now();
        let q = self.inputs.lock().unwrap();
        let result = if (button as usize) < q.mouse_down.len() {
            q.mouse_down[button as usize]
        } else {
            false
        };
        drop(q);
        self.abi_end(_t);
        result
    }
    fn mouse_x(&mut self) -> f32 {
        let _t = Instant::now();
        let result = self.inputs.lock().unwrap().mouse_x;
        self.abi_end(_t);
        result
    }
    fn mouse_y(&mut self) -> f32 {
        let _t = Instant::now();
        let result = self.inputs.lock().unwrap().mouse_y;
        self.abi_end(_t);
        result
    }
    /// Single-call input snapshot: 41 bytes replacing 262 individual ABI calls.
    /// [0..32]  4×u64 key bitmask for codes 0–255 (little-endian)
    /// [32]     mouse-buttons bitmask (bit 0=left, 1=right, 2=middle)
    /// [33..37] mouse-x f32 LE
    /// [37..41] mouse-y f32 LE
    fn get_input_snapshot(&mut self) -> Vec<u8> {
        let _t = Instant::now();
        let q = self.inputs.lock().unwrap();
        let mut buf = vec![0u8; 41];
        // Build 4 u64 words covering key codes 0..255
        let mut words = [0u64; 4];
        for &code in &q.keys_down {
            if (code as usize) < 256 {
                words[code as usize / 64] |= 1u64 << (code % 64);
            }
        }
        for (i, w) in words.iter().enumerate() {
            buf[i * 8..i * 8 + 8].copy_from_slice(&w.to_le_bytes());
        }
        // Mouse buttons bitmask
        let mut mouse_mask: u8 = 0;
        for (i, &down) in q.mouse_down.iter().enumerate().take(8) {
            if down { mouse_mask |= 1 << i; }
        }
        buf[32] = mouse_mask;
        buf[33..37].copy_from_slice(&q.mouse_x.to_le_bytes());
        buf[37..41].copy_from_slice(&q.mouse_y.to_le_bytes());
        drop(q);
        self.abi_end(_t);
        buf
    }
}

impl ube::graphics::manifest::Host for HostState {
    fn manifest_width(&mut self) -> u32 {
        let _t = Instant::now();
        let result = self.manifest_width;
        self.abi_end(_t);
        result
    }
    fn manifest_height(&mut self) -> u32 {
        let _t = Instant::now();
        let result = self.manifest_height;
        self.abi_end(_t);
        result
    }
}

impl ube::graphics::gpu::Host for HostState {
    fn setup(&mut self, width: u32, height: u32) {
        let _t = Instant::now();
        self.gpu.resize(width.max(1), height.max(1));
        self.abi_end(_t);
    }

    fn set_vertices(&mut self, data: Vec<f32>) {
        let _t = Instant::now();
        self.gpu.upload_vertices(&data);
        self.abi_end(_t);
    }

    fn draw_frame(&mut self, r: f64, g: f64, b: f64) {
        // present_frame resets abi counters itself; just forward
        self.present_frame(r, g, b);
    }
}

impl ube::graphics::gpu3d::Host for HostState {
    fn upload_mesh(&mut self, vertices: Vec<f32>, indices: Vec<u32>) -> u32 {
        let _t = Instant::now();
        let result = self.gpu.upload_mesh(&vertices, &indices);
        self.abi_end(_t);
        result
    }

    fn destroy_mesh(&mut self, id: u32) {
        let _t = Instant::now();
        self.gpu.destroy_mesh(id);
        self.abi_end(_t);
    }

    fn set_camera(&mut self, view_proj: Vec<f32>, eye_pos: Vec<f32>) {
        let _t = Instant::now();
        self.gpu.set_camera_3d(&view_proj, &eye_pos);
        self.abi_end(_t);
    }

    fn set_directional_light(&mut self, dir: Vec<f32>, color: Vec<f32>, intensity: f32) {
        let _t = Instant::now();
        self.gpu.set_light_3d(&dir, &color, intensity);
        self.abi_end(_t);
    }

    fn draw_instances(&mut self, mesh: u32, instance_data: Vec<f32>) {
        let _t = Instant::now();
        self.gpu.draw_instances_3d(mesh, &instance_data);
        self.abi_end(_t);
    }

    fn draw_frame(&mut self, clear_r: f64, clear_g: f64, clear_b: f64) {
        // present_frame resets abi counters itself; just forward
        self.present_frame(clear_r, clear_g, clear_b);
    }
}

impl ube::graphics::trace::Host for HostState {
    fn log(&mut self, level: u8, tag: String, msg: String) {
        let _t = Instant::now();
        let lvl_str: &'static str = match level {
            0 => "INFO",
            1 => "WARN",
            2 => "ERR",
            _ => "LOG",
        };
        let log_line = format!("{}|f:{}|+0us|cat:guest|{}|msg:{}", lvl_str, self.frame_index, tag, msg);
        if let Some(ref telem) = self.telemetry {
            telem.publish_event(log_line.clone());
        }
        if let Some(ref mut rec) = self.recorder {
            rec.push_event(self.frame_index, 0, lvl_str, "guest", tag, format!("msg:{}", msg));
        }
        self.abi_end(_t);
    }

    fn span_begin(&mut self, name: String) {
        let _t = Instant::now();
        if let Some(ref mut rec) = self.recorder {
            rec.open_span_stack.push((name, Instant::now()));
        }
        self.abi_end(_t);
    }

    fn span_end(&mut self) {
        let _t = Instant::now();
        if let Some(ref mut rec) = self.recorder {
            if let Some((name, start)) = rec.open_span_stack.pop() {
                let duration_us = start.elapsed().as_micros() as u32;
                let frame_idx = self.frame_index;
                rec.push_event(frame_idx, 0, "SPAN", "guest", name.clone(), format!("dur_us:{}", duration_us));
                let span_line = format!("SPAN|f:{}|+0us|cat:guest|{}|dur_us:{}", frame_idx, name, duration_us);
                if let Some(ref telem) = self.telemetry {
                    telem.publish_event(span_line);
                }
            }
        }
        self.abi_end(_t);
    }

    fn metric_u32(&mut self, name: String, value: u32) {
        let _t = Instant::now();
        let frame_idx = self.frame_index;
        let metric_line = format!("METRIC|f:{}|+0us|cat:guest|{}|val:{}", frame_idx, name, value);
        if let Some(ref telem) = self.telemetry {
            telem.publish_event(metric_line);
        }
        if let Some(ref mut rec) = self.recorder {
            rec.push_event(frame_idx, 0, "INFO", "guest", name, format!("val:{}", value));
        }
        self.abi_end(_t);
    }

    fn delta_time(&mut self) -> f32 {
        let _t = Instant::now();
        let result = self.last_dt_secs;
        self.abi_end(_t);
        result
    }
}

unsafe impl Send for HostState {}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <game-component.wasm>", args[0]);
        std::process::exit(1);
    }
    let wasm_path = &args[1];

    let mut config = Config::new();
    config.wasm_component_model(true);
    let engine = Engine::new(&config)?;

    let event_loop = EventLoop::new()?;
    let event_loop_cell = Arc::new(SendEventLoop(Mutex::new(Some(event_loop))));

    let mut linker: Linker<HostState> = Linker::new(&engine);
    wasmtime_wasi::add_to_linker_sync(&mut linker)?;
    ube::graphics::window::add_to_linker(&mut linker, |state: &mut HostState| state)?;
    ube::graphics::gpu::add_to_linker(&mut linker, |state: &mut HostState| state)?;
    ube::graphics::gpu3d::add_to_linker(&mut linker, |state: &mut HostState| state)?;
    ube::graphics::input::add_to_linker(&mut linker, |state: &mut HostState| state)?;
    ube::graphics::trace::add_to_linker(&mut linker, |state: &mut HostState| state)?;
    ube::graphics::manifest::add_to_linker(&mut linker, |state: &mut HostState| state)?;

    let wasi = WasiCtxBuilder::new()
        .inherit_stdout()
        .inherit_stderr()
        .inherit_stdin()
        .build();

    let inputs = Arc::new(Mutex::new(InputQueue::new()));
    let target_fps = env::var("GROOT_FPS")
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
        .filter(|fps| *fps > 0.0)
        .unwrap_or(60.0);
    let frame_interval = Duration::from_secs_f64(1.0 / target_fps);

    // Start telemetry IPC server in debug mode (GROOT_MODE != "release").
    let debug_mode = env::var("GROOT_MODE")
        .map(|v| v != "release")
        .unwrap_or(true);
    let project_name = env::var("GROOT_PROJECT").unwrap_or_else(|_| "groot".into());
    let telemetry = if debug_mode {
        TelemetryServer::start(&project_name)
    } else {
        None
    };

    let trace_file = env::var("GROOT_TRACE_FILE").ok();
    let vsync = env::var("GROOT_VSYNC")
        .map(|v| v != "false" && v != "0")
        .unwrap_or(true);

    let recorder = if debug_mode {
        Some(FlightRecorder::new(
            project_name.clone(),
            target_fps,
            vsync,
            trace_file.as_deref(),
        ))
    } else {
        None
    };

    // Manifest values injected by the groot CLI via environment variables.
    let manifest_width  = env::var("GROOT_WIDTH").ok()
        .and_then(|v| v.parse::<u32>().ok()).unwrap_or(1280);
    let manifest_height = env::var("GROOT_HEIGHT").ok()
        .and_then(|v| v.parse::<u32>().ok()).unwrap_or(720);
    let manifest_title  = env::var("GROOT_TITLE")
        .unwrap_or_else(|_| "Groot Game".into());

    let mut store = Store::new(
        &engine,
        HostState {
            window_mgr: WindowManager::new(),
            gpu: GpuRenderer::new(),
            inputs: Arc::clone(&inputs),
            wasi,
            table: ResourceTable::new(),
            event_loop: Arc::clone(&event_loop_cell),
            frame_interval,
            next_frame: None,
            close_requested: false,
            vsync,
            in_event_loop: false,
            telemetry,
            recorder,
            frame_index: 0,
            frame_start: None,
            last_guest_end: None,
            frame_abi_us: 0,
            frame_abi_calls: 0,
            last_dt_secs: 0.016666,
            manifest_width,
            manifest_height,
            manifest_title,
        },
    );

    let component = Component::from_file(&engine, wasm_path)?;
    let instance = linker.instantiate(&mut store, &component)?;
    let runner_world = GrootRunner::new(&mut store, &instance)?;

    // Check if component exports `tick(dt: f32) -> bool` or `tick()`
    let tick_fn_with_dt = instance.get_typed_func::<(f32,), (bool,)>(&mut store, "tick").ok();
    let tick_fn_void = if tick_fn_with_dt.is_none() {
        instance.get_typed_func::<(), ()>(&mut store, "tick").ok()
    } else {
        None
    };
    let has_tick = tick_fn_with_dt.is_some() || tick_fn_void.is_some();

    // Call init if present
    if let Ok(init_fn) = instance.get_typed_func::<(), ()>(&mut store, "init") {
        let _ = init_fn.call(&mut store, ());
        let _ = init_fn.post_return(&mut store);
    }

    if has_tick {
        log::info!("Component exports tick(); driving inverted host frame loop");
        // Also call run if present (for initial window setup) or let tick do it
        let _ = runner_world.wasi_cli_run().call_run(&mut store);
    } else {
        // Fallback: guest drives its own synchronous main loop
        match runner_world.wasi_cli_run().call_run(&mut store) {
            Ok(Ok(())) => {}
            Ok(Err(())) => eprintln!("[groot-host] guest returned an error"),
            Err(err) => return Err(err.into()),
        }
    }

    if !store.data().window_mgr.is_open() {
        log::info!("guest did not open a window; nothing to present");
        return Ok(());
    }

    // If guest ran synchronously (has_tick == false), the synchronous loop has finished.
    // Exit cleanly without falling into a redundant, frozen event loop.
    if !has_tick {
        return Ok(());
    }

    let event_loop = event_loop_cell
        .0
        .lock()
        .unwrap()
        .take()
        .expect("event loop already taken");

    let mut last_tick_time = Instant::now();
    let mut is_running = true;
    store.data_mut().in_event_loop = true;

    event_loop.run(move |event, target| {
        target.set_control_flow(ControlFlow::Poll);
        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => target.exit(),

            Event::WindowEvent {
                event: WindowEvent::Resized(size),
                ..
            } => store.data_mut().gpu.resize(size.width, size.height),

            Event::WindowEvent {
                event: WindowEvent::RedrawRequested,
                ..
            } => {
                if has_tick && is_running {
                    let now = Instant::now();
                    let dt = now.duration_since(last_tick_time).as_secs_f32();
                    last_tick_time = now;

                    if let Some(ref t_fn) = tick_fn_with_dt {
                        match t_fn.call(&mut store, (dt,)) {
                            Ok((keep_running,)) => {
                                let _ = t_fn.post_return(&mut store);
                                if !keep_running {
                                    is_running = false;
                                }
                            }
                            Err(e) => {
                                eprintln!("[groot-host] guest tick error: {e}");
                                is_running = false;
                            }
                        }
                    } else if let Some(ref t_fn) = tick_fn_void {
                        match t_fn.call(&mut store, ()) {
                            Ok(()) => {
                                let _ = t_fn.post_return(&mut store);
                            }
                            Err(e) => {
                                eprintln!("[groot-host] guest tick error: {e}");
                                is_running = false;
                            }
                        }
                    }
                } else {
                    store.data_mut().gpu.redraw();
                }
            }

            Event::WindowEvent {
                event: WindowEvent::CursorMoved { position, .. },
                ..
            } => {
                let mut q = store.data().inputs.lock().unwrap();
                q.set_mouse(position.x as f32, position.y as f32);
                q.push(RawInputEvent {
                    event_type: 2,
                    key_code: 0,
                    mouse_x: position.x as f32,
                    mouse_y: position.y as f32,
                });
            }

            Event::WindowEvent {
                event: WindowEvent::MouseInput { state, button, .. },
                ..
            } => {
                let kind = if state == ElementState::Pressed { 3 } else { 4 };
                let is_pressed = state == ElementState::Pressed;
                let code = match button {
                    MouseButton::Left => 1,
                    MouseButton::Right => 2,
                    MouseButton::Middle => 3,
                    _ => 0,
                };
                let mut q = store.data().inputs.lock().unwrap();
                q.on_mouse_button(code, is_pressed);
                let (mx, my) = (q.mouse_x, q.mouse_y);
                q.push(RawInputEvent {
                    event_type: kind,
                    key_code: code,
                    mouse_x: mx,
                    mouse_y: my,
                });
            }

            Event::WindowEvent {
                event: WindowEvent::KeyboardInput { event, .. },
                ..
            } => {
                let kind = if event.state == ElementState::Pressed { 0 } else { 1 };
                let is_pressed = event.state == ElementState::Pressed;
                let code = match event.physical_key {
                    PhysicalKey::Code(code) => key_code_to_u32(code),
                    _ => 0,
                };
                let mut q = store.data().inputs.lock().unwrap();
                q.on_key(code, is_pressed);
                let (mx, my) = (q.mouse_x, q.mouse_y);
                q.push(RawInputEvent {
                    event_type: kind,
                    key_code: code,
                    mouse_x: mx,
                    mouse_y: my,
                });
            }

            Event::AboutToWait => {
                if let Some(win) = store.data().window_mgr.window() {
                    win.request_redraw();
                }
            }
            _ => (),
        }
    })?;

    Ok(())
}

/// Stable numeric codes for the Ube `InputManager` (USB HID-ish values for
/// letters/digits; winit's enum otherwise).
fn key_code_to_u32(code: KeyCode) -> u32 {
    match code {
        KeyCode::Space => 32,
        KeyCode::Enter => 13,
        KeyCode::Escape => 27,
        KeyCode::Tab => 9,
        KeyCode::Backspace => 8,
        KeyCode::ArrowLeft => 37,
        KeyCode::ArrowUp => 38,
        KeyCode::ArrowRight => 39,
        KeyCode::ArrowDown => 40,
        KeyCode::KeyA => 65,
        KeyCode::KeyB => 66,
        KeyCode::KeyC => 67,
        KeyCode::KeyD => 68,
        KeyCode::KeyE => 69,
        KeyCode::KeyF => 70,
        KeyCode::KeyG => 71,
        KeyCode::KeyH => 72,
        KeyCode::KeyI => 73,
        KeyCode::KeyJ => 74,
        KeyCode::KeyK => 75,
        KeyCode::KeyL => 76,
        KeyCode::KeyM => 77,
        KeyCode::KeyN => 78,
        KeyCode::KeyO => 79,
        KeyCode::KeyP => 80,
        KeyCode::KeyQ => 81,
        KeyCode::KeyR => 82,
        KeyCode::KeyS => 83,
        KeyCode::KeyT => 84,
        KeyCode::KeyU => 85,
        KeyCode::KeyV => 86,
        KeyCode::KeyW => 87,
        KeyCode::KeyX => 88,
        KeyCode::KeyY => 89,
        KeyCode::KeyZ => 90,
        KeyCode::Digit0 => 48,
        KeyCode::Digit1 => 49,
        KeyCode::Digit2 => 50,
        KeyCode::Digit3 => 51,
        KeyCode::Digit4 => 52,
        KeyCode::Digit5 => 53,
        KeyCode::Digit6 => 54,
        KeyCode::Digit7 => 55,
        KeyCode::Digit8 => 56,
        KeyCode::Digit9 => 57,
        _ => 0,
    }
}
