use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{LazyLock, Mutex};

use bevy::prelude::*;
use goscript::{HotReloadEngine, Value};

use crate::groot_module::GrootModuleExt;

// ---------------------------------------------------------------------------
// Components & Resources
// ---------------------------------------------------------------------------

/// Marker component for Bevy entities driven by a GoScript file.
#[derive(Component, Clone, Debug)]
#[allow(dead_code)]
pub struct GoScriptComponent {
    pub script_path: String,
    pub entity_id: u32,
    pub tag: String,
}

/// Debug drawing command queued by `groot.DrawDebug*` calls.
pub enum GizmoCommand {
    Line(Vec2, Vec2, Color),
    Circle(Vec2, f32, Color),
    Rect(Vec2, Vec2, Color),
}

/// Entity spawn request queued by `groot.SpawnEntity`.
pub struct SpawnRequest {
    pub script: String,
    pub x: f32,
    pub y: f32,
}

/// Custom gameplay event emitted by `groot.EmitEvent`.
pub struct ScriptEvent {
    pub name: String,
    pub payload: f64,
}

/// Per-entity runtime state managed by the plugin, read/written by scripts.
#[derive(Debug, Clone, Copy, Default)]
pub struct EntityState {
    pub x: f32,
    pub y: f32,
    pub rotation: f32,
    pub scale_x: f32,
    pub scale_y: f32,
    pub color: (f32, f32, f32, f32),
    pub destroy_requested: bool,
}

// ---------------------------------------------------------------------------
// Thread-local entity context
// ---------------------------------------------------------------------------

// The entity ID currently being executed by the VM. Set before each OnUpdate
// call so that groot.GetSelf* / groot.SetSelf* operate on the correct entity.
thread_local! {
    static CURRENT_ENTITY: Cell<u32> = const { Cell::new(0) };
}

// ---------------------------------------------------------------------------
// Global state slots (cross-boundary between script VM and Bevy ECS)
// ---------------------------------------------------------------------------

static ENTITY_STATES: LazyLock<Mutex<HashMap<u32, EntityState>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static GIZMO_COMMANDS: Mutex<Vec<GizmoCommand>> = Mutex::new(Vec::new());
static SPAWN_REQUESTS: Mutex<Vec<SpawnRequest>> = Mutex::new(Vec::new());
static SCRIPT_EVENTS: Mutex<Vec<ScriptEvent>> = Mutex::new(Vec::new());

// ---------------------------------------------------------------------------
// Shared input state
// ---------------------------------------------------------------------------

struct InputState {
    move_x: Cell<f64>,
    move_y: Cell<f64>,
    keys_down: RefCell<Vec<String>>,
    keys_just_pressed: RefCell<Vec<String>>,
    mouse_pos: Cell<(f64, f64)>,
    mouse_button_down: Cell<[bool; 3]>,
    mouse_button_pressed: Cell<[bool; 3]>,
}

// ---------------------------------------------------------------------------
// GrootScriptHost — one engine per script file, shared across entities
// ---------------------------------------------------------------------------

pub struct GrootScriptHost {
    engines: HashMap<String, HotReloadEngine>,
    input: Rc<InputState>,
}

impl GrootScriptHost {
    fn new() -> Self {
        Self {
            engines: HashMap::new(),
            input: Rc::new(InputState {
                move_x: Cell::new(0.0),
                move_y: Cell::new(0.0),
                keys_down: RefCell::new(Vec::new()),
                keys_just_pressed: RefCell::new(Vec::new()),
                mouse_pos: Cell::new((0.0, 0.0)),
                mouse_button_down: Cell::new([false; 3]),
                mouse_button_pressed: Cell::new([false; 3]),
            }),
        }
    }

    fn ensure_engine(&mut self, script_path: &str) -> &mut HotReloadEngine {
        self.engines
            .entry(script_path.to_string())
            .or_insert_with(|| {
                let mut engine = HotReloadEngine::new(script_path);
                let vm = &mut engine.vm;

                // 1. Stateless utilities (math, collision, logging)
                vm.register_groot_module();

                // 2. Entity context API (Self*)
                let inp_axis = Rc::clone(&self.input);
                vm.register_fn("groot.GetAxis", move |args| {
                    if let Some(Value::String(axis)) = args.first() {
                        match axis.as_str() {
                            "Horizontal" => return Value::Float(inp_axis.move_x.get()),
                            "Vertical" => return Value::Float(inp_axis.move_y.get()),
                            _ => {}
                        }
                    }
                    Value::Float(0.0)
                });

                let inp_key = Rc::clone(&self.input);
                vm.register_fn("groot.IsKeyDown", move |args| {
                    if let Some(key) = args.first().and_then(|v| v.as_string()) {
                        return Value::Bool(inp_key.keys_down.borrow().iter().any(|k| k == key));
                    }
                    Value::Bool(false)
                });

                let inp_pressed = Rc::clone(&self.input);
                vm.register_fn("groot.IsKeyPressed", move |args| {
                    if let Some(key) = args.first().and_then(|v| v.as_string()) {
                        return Value::Bool(
                            inp_pressed.keys_just_pressed.borrow().iter().any(|k| k == key),
                        );
                    }
                    Value::Bool(false)
                });

                let inp_mouse = Rc::clone(&self.input);
                vm.register_fn("groot.GetMousePosition", move |_| {
                    let (mx, my) = inp_mouse.mouse_pos.get();
                    Value::Slice(Rc::new(RefCell::new(vec![
                        Value::Float(mx),
                        Value::Float(my),
                    ])))
                });

                let inp_mdown = Rc::clone(&self.input);
                vm.register_fn("groot.IsMouseButtonDown", move |args| {
                    let btn = args.first().and_then(|v| v.as_number()).unwrap_or(0.0) as usize;
                    Value::Bool(inp_mdown.mouse_button_down.get()[btn.min(2)])
                });

                let inp_mpressed = Rc::clone(&self.input);
                vm.register_fn("groot.IsMouseButtonPressed", move |args| {
                    let btn = args.first().and_then(|v| v.as_number()).unwrap_or(0.0) as usize;
                    Value::Bool(inp_mpressed.mouse_button_pressed.get()[btn.min(2)])
                });

                // Self-context: position, rotation, scale, color, destroy
                vm.register_fn("groot.GetSelfEntity", |_| {
                    let id = CURRENT_ENTITY.with(|c| c.get());
                    Value::Int(id as i64)
                });

                vm.register_fn("groot.GetSelfPosition", |_| {
                    let id = CURRENT_ENTITY.with(|c| c.get());
                    let lock = ENTITY_STATES.lock().unwrap();
                    let s = lock.get(&id).cloned().unwrap_or_default();
                    Value::Slice(Rc::new(RefCell::new(vec![
                        Value::Float(s.x as f64),
                        Value::Float(s.y as f64),
                    ])))
                });

                vm.register_fn("groot.SetSelfPosition", |args| {
                    let id = CURRENT_ENTITY.with(|c| c.get());
                    let nx = args.get(0).and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
                    let ny = args.get(1).and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
                    let mut lock = ENTITY_STATES.lock().unwrap();
                    let s = lock.entry(id).or_default();
                    s.x = nx;
                    s.y = ny;
                    Value::Nil
                });

                vm.register_fn("groot.GetSelfRotation", |_| {
                    let id = CURRENT_ENTITY.with(|c| c.get());
                    let lock = ENTITY_STATES.lock().unwrap();
                    let s = lock.get(&id).cloned().unwrap_or_default();
                    Value::Float(s.rotation as f64)
                });

                vm.register_fn("groot.SetSelfRotation", |args| {
                    let id = CURRENT_ENTITY.with(|c| c.get());
                    let rot = args.get(0).and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
                    let mut lock = ENTITY_STATES.lock().unwrap();
                    let s = lock.entry(id).or_default();
                    s.rotation = rot;
                    Value::Nil
                });

                vm.register_fn("groot.GetSelfScale", |_| {
                    let id = CURRENT_ENTITY.with(|c| c.get());
                    let lock = ENTITY_STATES.lock().unwrap();
                    let s = lock.get(&id).cloned().unwrap_or_default();
                    Value::Slice(Rc::new(RefCell::new(vec![
                        Value::Float(s.scale_x as f64),
                        Value::Float(s.scale_y as f64),
                    ])))
                });

                vm.register_fn("groot.SetSelfScale", |args| {
                    let id = CURRENT_ENTITY.with(|c| c.get());
                    let sx = args.get(0).and_then(|v| v.as_number()).unwrap_or(1.0) as f32;
                    let sy = args.get(1).and_then(|v| v.as_number()).unwrap_or(1.0) as f32;
                    let mut lock = ENTITY_STATES.lock().unwrap();
                    let s = lock.entry(id).or_default();
                    s.scale_x = sx;
                    s.scale_y = sy;
                    Value::Nil
                });

                vm.register_fn("groot.SetSelfColor", |args| {
                    let id = CURRENT_ENTITY.with(|c| c.get());
                    let r = args.get(0).and_then(|v| v.as_number()).unwrap_or(1.0) as f32;
                    let g = args.get(1).and_then(|v| v.as_number()).unwrap_or(1.0) as f32;
                    let b = args.get(2).and_then(|v| v.as_number()).unwrap_or(1.0) as f32;
                    let a = args.get(3).and_then(|v| v.as_number()).unwrap_or(1.0) as f32;
                    let mut lock = ENTITY_STATES.lock().unwrap();
                    let s = lock.entry(id).or_default();
                    s.color = (r, g, b, a);
                    Value::Nil
                });

                vm.register_fn("groot.DestroySelf", |_| {
                    let id = CURRENT_ENTITY.with(|c| c.get());
                    let mut lock = ENTITY_STATES.lock().unwrap();
                    if let Some(s) = lock.get_mut(&id) {
                        s.destroy_requested = true;
                    }
                    Value::Nil
                });

                // Entity queries
                vm.register_fn("groot.GetEntityPosition", |args| {
                    let tid = args.first().and_then(|v| v.as_number()).unwrap_or(0.0) as u32;
                    let lock = ENTITY_STATES.lock().unwrap();
                    let s = lock.get(&tid).cloned().unwrap_or_default();
                    Value::Slice(Rc::new(RefCell::new(vec![
                        Value::Float(s.x as f64),
                        Value::Float(s.y as f64),
                    ])))
                });

                vm.register_fn("groot.GetDistance", |args| {
                    let id1 = args.get(0).and_then(|v| v.as_number()).unwrap_or(0.0) as u32;
                    let id2 = args.get(1).and_then(|v| v.as_number()).unwrap_or(0.0) as u32;
                    let lock = ENTITY_STATES.lock().unwrap();
                    let s1 = lock.get(&id1).cloned().unwrap_or_default();
                    let s2 = lock.get(&id2).cloned().unwrap_or_default();
                    let dx = s2.x - s1.x;
                    let dy = s2.y - s1.y;
                    Value::Float((dx * dx + dy * dy).sqrt() as f64)
                });

                // Debug drawing
                vm.register_fn("groot.DrawDebugLine", |args| {
                    if args.len() >= 4 {
                        let x1 = args[0].as_number().unwrap_or(0.0) as f32;
                        let y1 = args[1].as_number().unwrap_or(0.0) as f32;
                        let x2 = args[2].as_number().unwrap_or(0.0) as f32;
                        let y2 = args[3].as_number().unwrap_or(0.0) as f32;
                        let r = args.get(4).and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
                        let g = args.get(5).and_then(|v| v.as_number()).unwrap_or(1.0) as f32;
                        let b = args.get(6).and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
                        if let Ok(mut cmds) = GIZMO_COMMANDS.lock() {
                            cmds.push(GizmoCommand::Line(
                                Vec2::new(x1, y1),
                                Vec2::new(x2, y2),
                                Color::rgb(r, g, b),
                            ));
                        }
                    }
                    Value::Nil
                });

                vm.register_fn("groot.DrawDebugCircle", |args| {
                    if args.len() >= 3 {
                        let cx = args[0].as_number().unwrap_or(0.0) as f32;
                        let cy = args[1].as_number().unwrap_or(0.0) as f32;
                        let radius = args[2].as_number().unwrap_or(10.0) as f32;
                        let r = args.get(3).and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
                        let g = args.get(4).and_then(|v| v.as_number()).unwrap_or(1.0) as f32;
                        let b = args.get(5).and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
                        if let Ok(mut cmds) = GIZMO_COMMANDS.lock() {
                            cmds.push(GizmoCommand::Circle(
                                Vec2::new(cx, cy),
                                radius,
                                Color::rgb(r, g, b),
                            ));
                        }
                    }
                    Value::Nil
                });

                vm.register_fn("groot.DrawDebugRect", |args| {
                    if args.len() >= 4 {
                        let cx = args[0].as_number().unwrap_or(0.0) as f32;
                        let cy = args[1].as_number().unwrap_or(0.0) as f32;
                        let w = args[2].as_number().unwrap_or(10.0) as f32;
                        let h = args[3].as_number().unwrap_or(10.0) as f32;
                        let r = args.get(4).and_then(|v| v.as_number()).unwrap_or(1.0) as f32;
                        let g = args.get(5).and_then(|v| v.as_number()).unwrap_or(1.0) as f32;
                        let b = args.get(6).and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
                        if let Ok(mut cmds) = GIZMO_COMMANDS.lock() {
                            cmds.push(GizmoCommand::Rect(
                                Vec2::new(cx, cy),
                                Vec2::new(w, h),
                                Color::rgb(r, g, b),
                            ));
                        }
                    }
                    Value::Nil
                });

                // Commands & Events
                vm.register_fn("groot.SpawnEntity", |args| {
                    let script = args
                        .first()
                        .and_then(|v| v.as_string())
                        .unwrap_or("")
                        .to_string();
                    let x = args.get(1).and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
                    let y = args.get(2).and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
                    if let Ok(mut reqs) = SPAWN_REQUESTS.lock() {
                        reqs.push(SpawnRequest { script, x, y });
                    }
                    Value::Nil
                });

                vm.register_fn("groot.PlaySound", |args| {
                    if let Some(name) = args.first().and_then(|v| v.as_string()) {
                        bevy::log::info!("[GROOT AUDIO]: Playing sound '{name}'");
                    }
                    Value::Nil
                });

                vm.register_fn("groot.EmitEvent", |args| {
                    let name = args.first().and_then(|v| v.as_string()).unwrap_or("");
                    let payload = args.get(1).and_then(|v| v.as_number()).unwrap_or(0.0);
                    if let Ok(mut events) = SCRIPT_EVENTS.lock() {
                        events.push(ScriptEvent {
                            name: name.to_string(),
                            payload,
                        });
                    }
                    Value::Nil
                });

                engine
            })
    }

    pub fn reload_all(&mut self) {
        for engine in self.engines.values_mut() {
            let _ = engine.reload_if_changed();
        }
    }

    pub fn tick_entity(&mut self, script_path: &str, entity_id: u32, dt: f64) {
        let engine = self.ensure_engine(script_path);
        CURRENT_ENTITY.with(|c| c.set(entity_id));
        engine.vm.set_delta_time(dt);
        if let Err(e) = engine.vm.call("OnUpdate", vec![Value::Float(dt)]) {
            bevy::log::error!("[GROOT SCRIPT] entity #{entity_id} OnUpdate error: {e}");
        }
    }
}

// ---------------------------------------------------------------------------
// Bevy Plugin
// ---------------------------------------------------------------------------

pub struct GrootPlugin;

impl Plugin for GrootPlugin {
    fn build(&self, app: &mut App) {
        app.insert_non_send_resource(GrootScriptHost::new())
            .add_systems(
                Update,
                (
                    sync_entity_states_system,
                    script_hot_reload_system,
                    script_update_system,
                    apply_script_changes_system,
                    render_debug_gizmos_system,
                    handle_spawn_requests_system,
                    handle_script_events_system,
                )
                    .chain(),
            );
    }
}

// ---------------------------------------------------------------------------
// Systems
// ---------------------------------------------------------------------------

/// Sync Bevy Transform/Sprite state into the shared ENTITY_STATES map
/// before scripts run, so GetSelfPosition reflects the latest ECS state.
fn sync_entity_states_system(
    query: Query<(&GoScriptComponent, &Transform, Option<&Sprite>)>,
) {
    let mut lock = ENTITY_STATES.lock().unwrap();
    for (comp, transform, sprite) in &query {
        let entry = lock.entry(comp.entity_id).or_default();
        entry.x = transform.translation.x;
        entry.y = transform.translation.y;
        let (_, _, rot) = transform.rotation.to_euler(EulerRot::XYZ);
        entry.rotation = rot;
        entry.scale_x = transform.scale.x;
        entry.scale_y = transform.scale.y;
        if let Some(sprite) = sprite {
            let c = sprite.color;
            entry.color = (c.r(), c.g(), c.b(), c.a());
        }
    }
}

fn script_hot_reload_system(mut host: NonSendMut<GrootScriptHost>) {
    host.reload_all();
}

/// Run OnUpdate for every script entity, setting CURRENT_ENTITY before each call.
fn script_update_system(
    mut host: NonSendMut<GrootScriptHost>,
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mouse_input: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    camera_q: Query<(&Camera, &GlobalTransform)>,
    time: Res<Time>,
    query: Query<&GoScriptComponent>,
) {
    let dt = time.delta_seconds() as f64;

    // Update input state once per frame
    {
        let inp = &host.input;

        let mut move_x = 0.0f64;
        let mut move_y = 0.0f64;
        if keyboard_input.pressed(KeyCode::KeyD) || keyboard_input.pressed(KeyCode::ArrowRight) {
            move_x += 1.0;
        }
        if keyboard_input.pressed(KeyCode::KeyA) || keyboard_input.pressed(KeyCode::ArrowLeft) {
            move_x -= 1.0;
        }
        if keyboard_input.pressed(KeyCode::KeyW) || keyboard_input.pressed(KeyCode::ArrowUp) {
            move_y += 1.0;
        }
        if keyboard_input.pressed(KeyCode::KeyS) || keyboard_input.pressed(KeyCode::ArrowDown) {
            move_y -= 1.0;
        }
        inp.move_x.set(move_x);
        inp.move_y.set(move_y);

        let mut down = inp.keys_down.borrow_mut();
        let mut pressed = inp.keys_just_pressed.borrow_mut();
        down.clear();
        pressed.clear();

        let key_map = [
            (KeyCode::Space, "Space"),
            (KeyCode::KeyQ, "KeyQ"),
            (KeyCode::KeyE, "KeyE"),
            (KeyCode::KeyR, "KeyR"),
            (KeyCode::KeyW, "KeyW"),
            (KeyCode::KeyA, "KeyA"),
            (KeyCode::KeyS, "KeyS"),
            (KeyCode::KeyD, "KeyD"),
        ];
        for (code, name) in key_map {
            if keyboard_input.pressed(code) {
                down.push(name.to_string());
            }
            if keyboard_input.just_pressed(code) {
                pressed.push(name.to_string());
            }
        }

        if let Ok(window) = windows.get_single() {
            if let Some(cursor) = window.cursor_position() {
                if let Ok((camera, cam_tf)) = camera_q.get_single() {
                    if let Some(world) = camera.viewport_to_world_2d(cam_tf, cursor) {
                        inp.mouse_pos
                            .set((world.x as f64, world.y as f64));
                    }
                }
            }
        }

        inp.mouse_button_down.set([
            mouse_input.pressed(MouseButton::Left),
            mouse_input.pressed(MouseButton::Right),
            mouse_input.pressed(MouseButton::Middle),
        ]);
        inp.mouse_button_pressed.set([
            mouse_input.just_pressed(MouseButton::Left),
            mouse_input.just_pressed(MouseButton::Right),
            mouse_input.just_pressed(MouseButton::Middle),
        ]);
    }

    // Tick each entity's script
    for comp in &query {
        host.tick_entity(&comp.script_path, comp.entity_id, dt);
    }
}

/// Apply destroy requests and transform changes from scripts back to Bevy ECS.
fn apply_script_changes_system(
    mut commands: Commands,
    mut query: Query<(Entity, &GoScriptComponent, &mut Transform, Option<&mut Sprite>)>,
) {
    let lock = ENTITY_STATES.lock().unwrap();
    for (entity, comp, mut transform, sprite) in &mut query {
        if let Some(state) = lock.get(&comp.entity_id) {
            if state.destroy_requested {
                bevy::log::info!("[GROOT ECS] Despawning entity #{}", comp.entity_id);
                commands.entity(entity).despawn_recursive();
                continue;
            }
            transform.translation.x = state.x;
            transform.translation.y = state.y;
            transform.rotation = Quat::from_rotation_z(state.rotation);
            transform.scale = Vec3::new(state.scale_x, state.scale_y, 1.0);
            if let Some(mut sprite) = sprite {
                sprite.color =
                    Color::rgba(state.color.0, state.color.1, state.color.2, state.color.3);
            }
        }
    }
}

/// Render debug gizmo commands accumulated during the frame.
fn render_debug_gizmos_system(mut gizmos: Gizmos) {
    if let Ok(mut cmds) = GIZMO_COMMANDS.lock() {
        for cmd in cmds.drain(..) {
            match cmd {
                GizmoCommand::Line(start, end, color) => {
                    gizmos.line_2d(start, end, color);
                }
                GizmoCommand::Circle(center, radius, color) => {
                    gizmos.circle_2d(center, radius, color);
                }
                GizmoCommand::Rect(center, size, color) => {
                    gizmos.rect_2d(center, 0.0, size, color);
                }
            }
        }
    }
}

/// Process entity spawn requests from scripts.
fn handle_spawn_requests_system(
    mut host: NonSendMut<GrootScriptHost>,
) {
    if let Ok(mut reqs) = SPAWN_REQUESTS.lock() {
        for req in reqs.drain(..) {
            bevy::log::info!(
                "[GROOT ECS] Spawning '{}' at ({}, {})",
                req.script,
                req.x,
                req.y
            );
            // Ensure the engine is loaded for the new script
            host.ensure_engine(&req.script);
            // NOTE: In a full implementation, this would also create an ECS
            // entity with GoScriptComponent. For now we log the spawn.
        }
    }
}

/// Process gameplay events emitted by scripts.
fn handle_script_events_system() {
    if let Ok(mut events) = SCRIPT_EVENTS.lock() {
        for ev in events.drain(..) {
            bevy::log::info!(
                "[GROOT EVENT] '{}' payload={:.2}",
                ev.name,
                ev.payload
            );
        }
    }
}
