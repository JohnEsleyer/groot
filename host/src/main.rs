mod gpu;
mod input;
mod window;

use gpu::GpuRenderer;
use input::{InputQueue, RawInputEvent};
use std::env;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use wasmtime::component::{Component, Linker, ResourceTable};
use wasmtime::{Config, Engine, Store};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiView};
use window::WindowManager;
use winit::event::{ElementState, Event, MouseButton, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};

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
            self.gpu.init_surface(win, width, height);
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

impl ube::graphics::gpu::Host for HostState {
    fn setup(&mut self, width: u32, height: u32) {
        self.gpu.resize(width.max(1), height.max(1));
    }

    fn set_vertices(&mut self, data: Vec<f32>) {
        self.gpu.upload_vertices(&data);
    }

    fn draw_frame(&mut self, r: f64, g: f64, b: f64) {
        // The guest owns the frame loop, so the host can only pace it at the
        // present boundary. Sleep until the next deadline; if the guest is
        // behind schedule, re-anchor instead of accumulating debt.
        let now = Instant::now();
        if let Some(target) = self.next_frame {
            if target > now {
                std::thread::sleep(target - now);
            }
        }
        self.next_frame = Some(Instant::now() + self.frame_interval);

        self.gpu.draw_frame(r, g, b);
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
    ube::graphics::input::add_to_linker(&mut linker, |state: &mut HostState| state)?;

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
        },
    );

    let component = Component::from_file(&engine, wasm_path)?;
    let runner_world = GrootRunner::instantiate(&mut store, &component, &linker)?;

    // The guest owns its main loop for as long as it wants. Once it returns,
    // the native event loop keeps the final uploaded frame alive and handles
    // resizing/input until the window is closed.
    match runner_world.wasi_cli_run().call_run(&mut store) {
        Ok(Ok(())) => {}
        Ok(Err(())) => eprintln!("[groot-host] guest returned an error"),
        Err(err) => return Err(err.into()),
    }

    if !store.data().window_mgr.is_open() {
        log::info!("guest did not open a window; nothing to present");
        return Ok(());
    }

    let event_loop = event_loop_cell
        .0
        .lock()
        .unwrap()
        .take()
        .expect("event loop already taken");

    event_loop.run(move |event, target| {
        target.set_control_flow(ControlFlow::Wait);
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
            } => store.data_mut().gpu.redraw(),

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
