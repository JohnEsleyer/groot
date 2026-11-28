mod gpu;
mod input;
mod telemetry;
mod window;

use gpu::GpuRenderer;
use input::{InputQueue, RawInputEvent};
use std::env;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use telemetry::{TelemetryPacket, TelemetryServer};
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
    // --- Telemetry (debug mode only) ---
    telemetry: Option<TelemetryServer>,
    frame_index: u64,
    frame_start: Option<Instant>,
    // --- Manifest values (from groot.toml, injected by groot CLI via env vars) ---
    manifest_width:  u32,
    manifest_height: u32,
    manifest_title:  String,
}

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
        let _ = self.pump_window_events();
        if self.close_requested {
            std::process::exit(0);
        }

        let now = Instant::now();
        let frame_time_us = self
            .frame_start
            .map(|s| s.elapsed().as_micros() as u32)
            .unwrap_or(16_666);
        self.frame_start = Some(now);

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

        if let Some(win) = self.window_mgr.window() {
            win.request_redraw();
        }

        // Publish live frame telemetry
        self.frame_index += 1;
        if let Some(ref telem) = self.telemetry {
            let fps = if frame_time_us > 0 {
                1_000_000.0 / frame_time_us as f32
            } else {
                60.0
            };
            let (mouse_x, mouse_y, mouse_mask, keys_count) = {
                let q = self.inputs.lock().unwrap();
                (q.mouse_x, q.mouse_y, q.mouse_buttons_mask(), q.pressed_keys_count())
            };
            telem.publish(TelemetryPacket {
                frame_index: self.frame_index,
                frame_time_us,
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
                ..Default::default()
            });
        }
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
        log::info!("creating window {}x{} '{}'", width, height, title);

        let el_guard = self.event_loop.0.lock().unwrap();
        let Some(el) = el_guard.as_ref() else {
            log::error!("create-window called after the event loop started");
            return;
        };

        self.window_mgr
            .create_window(el, width, height, &title, true);
        if let Some(win) = self.window_mgr.window() {
            self.gpu.init_surface(win, width, height, self.vsync);
        }
    }
}

impl ube::graphics::input::Host for HostState {
    fn is_key_down(&mut self, code: u32) -> bool {
        self.inputs.lock().unwrap().keys_down.contains(&code)
    }
    fn is_mouse_down(&mut self, button: u32) -> bool {
        let q = self.inputs.lock().unwrap();
        if (button as usize) < q.mouse_down.len() {
            q.mouse_down[button as usize]
        } else {
            false
        }
    }
    fn mouse_x(&mut self) -> f32 {
        self.inputs.lock().unwrap().mouse_x
    }
    fn mouse_y(&mut self) -> f32 {
        self.inputs.lock().unwrap().mouse_y
    }
}

impl ube::graphics::manifest::Host for HostState {
    fn manifest_width(&mut self) -> u32 {
        self.manifest_width
    }
    fn manifest_height(&mut self) -> u32 {
        self.manifest_height
    }
}

impl ube::graphics::gpu::Host for HostState {
    fn setup(&mut self, width: u32, height: u32) {
        self.gpu.resize(width.max(1), height.max(1));
    }

    fn set_vertices(&mut self, data: Vec<f32>) {
        self.gpu.upload_vertices(&data);
    }

    fn draw_frame(&mut self, r: f64, g: f64, b: f64) {
        self.present_frame(r, g, b);
    }
}

impl ube::graphics::gpu3d::Host for HostState {
    fn upload_mesh(&mut self, vertices: Vec<f32>, indices: Vec<u32>) -> u32 {
        self.gpu.upload_mesh(&vertices, &indices)
    }

    fn destroy_mesh(&mut self, id: u32) {
        self.gpu.destroy_mesh(id);
    }

    fn set_camera(&mut self, view_proj: Vec<f32>, eye_pos: Vec<f32>) {
        self.gpu.set_camera_3d(&view_proj, &eye_pos);
    }

    fn set_directional_light(&mut self, dir: Vec<f32>, color: Vec<f32>, intensity: f32) {
        self.gpu.set_light_3d(&dir, &color, intensity);
    }

    fn draw_instances(&mut self, mesh: u32, instance_data: Vec<f32>) {
        self.gpu.draw_instances_3d(mesh, &instance_data);
    }

    fn draw_frame(&mut self, clear_r: f64, clear_g: f64, clear_b: f64) {
        self.present_frame(clear_r, clear_g, clear_b);
    }
}

unsafe impl Send for HostState {}

struct SendEventLoop(Mutex<Option<EventLoop<()>>>);
unsafe impl Send for SendEventLoop {}
unsafe impl Sync for SendEventLoop {}

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
    ube::graphics::manifest::add_to_linker(&mut linker, |state: &mut HostState| state)?;

    let wasi = WasiCtxBuilder::new()
        .inherit_stdout()
        .inherit_stderr()
        .inherit_stdin()
        .build();

    let inputs = Arc::new(Mutex::new(InputQueue::new()));
    let frame_interval = Duration::from_secs_f64(
        1.0 / env::var("GROOT_FPS")
            .ok()
            .and_then(|v| v.parse::<f64>().ok())
            .filter(|fps| *fps > 0.0)
            .unwrap_or(60.0),
    );

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

    // Manifest values injected by the groot CLI via environment variables.
    // These are read by app_new_from_manifest() through the manifest ABI.
    let manifest_width  = env::var("GROOT_WIDTH").ok()
        .and_then(|v| v.parse::<u32>().ok()).unwrap_or(1280);
    let manifest_height = env::var("GROOT_HEIGHT").ok()
        .and_then(|v| v.parse::<u32>().ok()).unwrap_or(720);
    let manifest_title  = env::var("GROOT_TITLE")
        .unwrap_or_else(|_| "Groot Game".into());

    let vsync = env::var("GROOT_VSYNC")
        .map(|v| v != "false" && v != "0")
        .unwrap_or(true);

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
            telemetry,
            frame_index: 0,
            frame_start: None,
            manifest_width,
            manifest_height,
            manifest_title,
        },
    );

    let component = Component::from_file(&engine, wasm_path)?;
    let (runner_world, instance) =
        GrootRunner::instantiate(&mut store, &component, &linker)
            .map(|rw| {
                // Also get the low-level instance to check for tick/init exports
                (rw, None)
            })
            .or_else(|_| {
                linker
                    .instantiate(&mut store, &component)
                    .map(|inst| (GrootRunner::new(&mut store, &inst).unwrap(), Some(inst)))
            })?;

    let instance = instance.unwrap_or_else(|| {
        // If instantiated via GrootRunner, also instantiate or query
        linker.instantiate(&mut store, &component).unwrap()
    });

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
        other => other as u32,
    }
}
