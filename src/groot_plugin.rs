use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Mutex;

use bevy::prelude::*;
use goscript::{HotReloadEngine, Value};

use crate::groot_module::GrootModuleExt;

// ---------------------------------------------------------------------------
// Components & Resources
// ---------------------------------------------------------------------------

/// Component driving entity behavior via GoScript. Entities with this
/// component get `OnUpdate(dt)` called every frame by the execution system.
#[derive(Component, Clone, Debug)]
#[allow(dead_code)] // `tag` is metadata for tooling/future query filtering.
pub struct GoScriptComponent {
    pub script_path: String,
    pub entity_id: u32,
    pub tag: String,
}

/// Marker for the Flappy Bird entity (the script entity that owns flappy.gs).
#[derive(Component)]
pub struct Bird;

/// Identifies a pipe segment by index (position set by `groot.SetPipePosition`).
#[derive(Component)]
pub struct PipeIndex(pub usize);

/// Marker for the on-screen score text entity.
#[derive(Component)]
pub struct ScoreText;

/// Native ECS transform state, kept as a component so scripts and systems can
/// read/write a script entity's position/rotation/scale without reaching for
/// the raw `Transform` across the VM boundary.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct ScriptTransform {
    pub x: f32,
    pub y: f32,
    pub rotation: f32,
    pub scale_x: f32,
    pub scale_y: f32,
}

/// Native ECS sprite color state for script entities.
#[derive(Component, Clone, Copy, Debug)]
pub struct ScriptColor(pub Color);

impl Default for ScriptColor {
    fn default() -> Self {
        Self(Color::WHITE)
    }
}

/// Debug drawing command queued by `groot.DrawDebug*` calls.
pub enum GizmoCommand {
    Line(Vec2, Vec2, Color),
    Circle(Vec2, f32, Color),
    Rect(Vec2, Vec2, Color),
}

/// Entity spawn request queued by `groot.SpawnEntity`.
#[derive(Clone, Debug)]
pub struct SpawnRequest {
    pub script: String,
    pub x: f32,
    pub y: f32,
    pub tag: String,
}

/// Custom gameplay event emitted by `groot.EmitEvent`.
#[derive(Clone, Debug)]
pub struct ScriptEvent {
    pub name: String,
    pub payload: f64,
}

/// Per-entity runtime state. This lives in a thread-local scratch slot for the
/// *currently executing* script entity rather than in a global map, so host
/// bindings (`groot.GetSelf*` / `groot.SetSelf*`) read and write the live
/// entity without locking a process-wide `Mutex<HashMap>`. The execution
/// system syncs it to/from the entity's Bevy `Transform`/`Sprite` components.
#[derive(Debug, Clone, Copy, Default)]
struct EntityState {
    x: f32,
    y: f32,
    rotation: f32,
    scale_x: f32,
    scale_y: f32,
    color: (f32, f32, f32, f32),
    destroy_requested: bool,
}

/// Flappy-style game writes queued by scripts and applied to tagged Bevy
/// entities by the execution system within the same frame.
#[derive(Debug, Clone, Copy)]
enum GameWrite {
    PipePosition(usize, f32, f32, f32),
    ScoreDisplay(i32, i32),
}

// ---------------------------------------------------------------------------
// Thread-local script context
// ---------------------------------------------------------------------------

// The script entity currently being executed. Set before each `OnUpdate` so
// the `groot.GetSelf*` / `groot.SetSelf*` bindings target the right entity.
thread_local! {
    static CURRENT_ENTITY: Cell<u32> = const { Cell::new(0) };
    static CURRENT_STATE: RefCell<EntityState> = RefCell::new(EntityState::default());

    // Frame snapshot of every scripted entity's position, rebuilt each frame
    // from ECS components so `groot.GetEntityPosition`/`groot.GetDistance` can
    // resolve sibling entities without a persistent global state map.
    static ENTITY_POSITIONS: RefCell<HashMap<u32, (f32, f32)>> = RefCell::new(HashMap::new());

    // Game-state writes queued by `groot.SetPipePosition`/`groot.SetScoreDisplay`
    // during the VM call and applied by the same system right after it returns.
    static GAME_WRITES: RefCell<Vec<GameWrite>> = const { RefCell::new(Vec::new()) };
}

// ---------------------------------------------------------------------------
// Cross-boundary command queues (script VM -> Bevy systems)
// ---------------------------------------------------------------------------

// These are intentionally kept as process-wide `static Mutex` buffers: scripts
// run inside the execution system but spawn requests / events / gizmos are
// drained by *other* systems in the schedule, and Bevy may schedule those on a
// different worker thread. Entity *state* no longer needs this — it flows
// through ECS components and the thread-local scratch above.
static SPAWN_REQUESTS: Mutex<Vec<SpawnRequest>> = Mutex::new(Vec::new());
static SCRIPT_EVENTS: Mutex<Vec<ScriptEvent>> = Mutex::new(Vec::new());
static GIZMO_COMMANDS: Mutex<Vec<GizmoCommand>> = Mutex::new(Vec::new());

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
    pub fn new() -> Self {
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

                // 2. Input API
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

                // 3. Self-context API. Reads/writes the thread-local scratch of
                // the currently executing entity; the execution system syncs it
                // to/from the entity's Bevy components around each OnUpdate.
                vm.register_fn("groot.GetSelfEntity", |_| {
                    let id = CURRENT_ENTITY.with(|c| c.get());
                    Value::Int(id as i64)
                });

                vm.register_fn("groot.GetSelfPosition", |_| {
                    let s = CURRENT_STATE.with(|st| *st.borrow());
                    Value::Slice(Rc::new(RefCell::new(vec![
                        Value::Float(s.x as f64),
                        Value::Float(s.y as f64),
                    ])))
                });

                vm.register_fn("groot.SetSelfPosition", |args| {
                    let nx = args.first().and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
                    let ny = args.get(1).and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
                    CURRENT_STATE.with(|st| {
                        let mut s = st.borrow_mut();
                        s.x = nx;
                        s.y = ny;
                    });
                    Value::Nil
                });

                // `groot.SetPosition` targets the script entity itself, so it is
                // an alias for `SetSelfPosition`. The Flappy demo uses it on the
                // bird entity, which owns its script.
                vm.register_fn("groot.SetPosition", |args| {
                    let nx = args.first().and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
                    let ny = args.get(1).and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
                    CURRENT_STATE.with(|st| {
                        let mut s = st.borrow_mut();
                        s.x = nx;
                        s.y = ny;
                    });
                    Value::Nil
                });

                vm.register_fn("groot.GetSelfRotation", |_| {
                    let s = CURRENT_STATE.with(|st| *st.borrow());
                    Value::Float(s.rotation as f64)
                });

                vm.register_fn("groot.SetSelfRotation", |args| {
                    let rot = args.first().and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
                    CURRENT_STATE.with(|st| st.borrow_mut().rotation = rot);
                    Value::Nil
                });

                vm.register_fn("groot.GetSelfScale", |_| {
                    let s = CURRENT_STATE.with(|st| *st.borrow());
                    Value::Slice(Rc::new(RefCell::new(vec![
                        Value::Float(s.scale_x as f64),
                        Value::Float(s.scale_y as f64),
                    ])))
                });

                vm.register_fn("groot.SetSelfScale", |args| {
                    let sx = args.first().and_then(|v| v.as_number()).unwrap_or(1.0) as f32;
                    let sy = args.get(1).and_then(|v| v.as_number()).unwrap_or(1.0) as f32;
                    CURRENT_STATE.with(|st| {
                        let mut s = st.borrow_mut();
                        s.scale_x = sx;
                        s.scale_y = sy;
                    });
                    Value::Nil
                });

                vm.register_fn("groot.SetSelfColor", |args| {
                    let r = args.first().and_then(|v| v.as_number()).unwrap_or(1.0) as f32;
                    let g = args.get(1).and_then(|v| v.as_number()).unwrap_or(1.0) as f32;
                    let b = args.get(2).and_then(|v| v.as_number()).unwrap_or(1.0) as f32;
                    let a = args.get(3).and_then(|v| v.as_number()).unwrap_or(1.0) as f32;
                    CURRENT_STATE.with(|st| st.borrow_mut().color = (r, g, b, a));
                    Value::Nil
                });

                vm.register_fn("groot.DestroySelf", |_| {
                    CURRENT_STATE.with(|st| st.borrow_mut().destroy_requested = true);
                    Value::Nil
                });

                // 4. Entity queries — resolved against the per-frame snapshot.
                vm.register_fn("groot.GetEntityPosition", |args| {
                    let tid = args.first().and_then(|v| v.as_number()).unwrap_or(0.0) as u32;
                    let pos = ENTITY_POSITIONS
                        .with(|p| p.borrow().get(&tid).copied())
                        .unwrap_or((0.0, 0.0));
                    Value::Slice(Rc::new(RefCell::new(vec![
                        Value::Float(pos.0 as f64),
                        Value::Float(pos.1 as f64),
                    ])))
                });

                vm.register_fn("groot.GetDistance", |args| {
                    let id1 = args.first().and_then(|v| v.as_number()).unwrap_or(0.0) as u32;
                    let id2 = args.get(1).and_then(|v| v.as_number()).unwrap_or(0.0) as u32;
                    let (p1, p2) = ENTITY_POSITIONS.with(|p| {
                        let map = p.borrow();
                        (
                            map.get(&id1).copied().unwrap_or((0.0, 0.0)),
                            map.get(&id2).copied().unwrap_or((0.0, 0.0)),
                        )
                    });
                    let dx = p2.0 - p1.0;
                    let dy = p2.1 - p1.1;
                    Value::Float((dx * dx + dy * dy).sqrt() as f64)
                });

                // 5. Debug drawing
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

                // 6. Commands & Events
                vm.register_fn("groot.SpawnEntity", |args| {
                    let script = args
                        .first()
                        .and_then(|v| v.as_string())
                        .unwrap_or("")
                        .to_string();
                    let x = args.get(1).and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
                    let y = args.get(2).and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
                    let tag = args.get(3).and_then(|v| v.as_string()).unwrap_or("").to_string();
                    if let Ok(mut reqs) = SPAWN_REQUESTS.lock() {
                        reqs.push(SpawnRequest { script, x, y, tag });
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

                // 7. Flappy-style game writes (routed to tagged ECS entities).
                vm.register_fn("groot.SetPipePosition", |args| {
                    let idx = args
                        .first()
                        .and_then(|v| v.as_number())
                        .unwrap_or(0.0) as usize;
                    let x = args.get(1).and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
                    let gap_y = args.get(2).and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
                    let gap_size = args.get(3).and_then(|v| v.as_number()).unwrap_or(130.0) as f32;
                    GAME_WRITES.with(|w| w.borrow_mut().push(GameWrite::PipePosition(idx, x, gap_y, gap_size)));
                    Value::Nil
                });

                vm.register_fn("groot.SetScoreDisplay", |args| {
                    let score = args.first().and_then(|v| v.as_number()).unwrap_or(0.0) as i32;
                    let high = args.get(1).and_then(|v| v.as_number()).unwrap_or(0.0) as i32;
                    GAME_WRITES.with(|w| w.borrow_mut().push(GameWrite::ScoreDisplay(score, high)));
                    Value::Nil
                });

                // Compile the script immediately so OnUpdate is defined on
                // the first frame (before script_hot_reload_system runs).
                let _ = engine.reload_if_changed();

                engine
            })
    }

    pub fn reload_all(&mut self) {
        for engine in self.engines.values_mut() {
            let _ = engine.reload_if_changed();
        }
    }
}

impl Default for GrootScriptHost {
    fn default() -> Self {
        Self::new()
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
                    script_hot_reload_system,
                    script_input_sync_system,
                    script_execution_system,
                    handle_spawn_requests_system,
                    handle_script_events_system,
                    render_debug_gizmos_system,
                )
                    .chain(),
            );
    }
}

// ---------------------------------------------------------------------------
// Systems
// ---------------------------------------------------------------------------

/// Recompile scripts whose source files changed on disk.
fn script_hot_reload_system(mut host: NonSendMut<GrootScriptHost>) {
    host.reload_all();
}

/// Refresh the input snapshot the script bindings read from, once per frame.
fn script_input_sync_system(
    host: NonSendMut<GrootScriptHost>,
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mouse_input: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    camera_q: Query<(&Camera, &GlobalTransform)>,
) {
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
                    inp.mouse_pos.set((world.x as f64, world.y as f64));
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

/// Query for entities driven by a GoScript file (the ones that receive
/// `OnUpdate` and whose self-state is synced each frame).
type ScriptEntityQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static GoScriptComponent,
        &'static mut Transform,
        Option<&'static mut Sprite>,
        &'static mut ScriptTransform,
        &'static mut ScriptColor,
    ),
>;

/// Query for script-positioned pipe sprites (no script component of their own).
type PipeEntityQuery<'w, 's> = Query<
    'w,
    's,
    (&'static mut Transform, &'static PipeIndex),
    (Without<GoScriptComponent>, Without<ScoreText>),
>;

/// Query for the score text entity updated by `groot.SetScoreDisplay`.
type ScoreTextQuery<'w, 's> = Query<
    'w,
    's,
    &'static mut Text,
    (With<ScoreText>, Without<GoScriptComponent>),
>;

/// Run `OnUpdate(dt)` for every script entity. The self-context bindings read
/// and write the thread-local [`EntityState`] scratch, which this system syncs
/// to/from the entity's `Transform`/`Sprite` components — so ECS components
/// remain the source of truth and no global state map is needed.
fn script_execution_system(
    mut host: NonSendMut<GrootScriptHost>,
    time: Res<Time>,
    mut commands: Commands,
    mut script_query: ScriptEntityQuery,
    mut pipe_query: PipeEntityQuery,
    mut score_query: ScoreTextQuery,
) {
    let dt = time.delta_seconds() as f64;

    // 1. Snapshot every scripted entity's position from ECS components so
    //    `groot.GetEntityPosition` / `groot.GetDistance` can resolve siblings.
    ENTITY_POSITIONS.with(|snapshot| {
        let mut map = snapshot.borrow_mut();
        map.clear();
        for (_, comp, transform, ..) in &script_query {
            map.insert(comp.entity_id, (transform.translation.x, transform.translation.y));
        }
    });

    // 2. Tick each entity's script.
    for (entity, comp, mut transform, sprite, mut script_tf, mut script_color) in &mut script_query {
        CURRENT_ENTITY.with(|c| c.set(comp.entity_id));

        // Sync ECS -> scratch so GetSelf* reflects the live entity.
        CURRENT_STATE.with(|st| {
            let mut s = st.borrow_mut();
            s.x = transform.translation.x;
            s.y = transform.translation.y;
            let (_, _, rot) = transform.rotation.to_euler(EulerRot::XYZ);
            s.rotation = rot;
            s.scale_x = transform.scale.x;
            s.scale_y = transform.scale.y;
            if let Some(sprite) = &sprite {
                let c = sprite.color;
                s.color = (c.r(), c.g(), c.b(), c.a());
            }
            s.destroy_requested = false;
        });

        let engine = host.ensure_engine(&comp.script_path);
        engine.vm.set_delta_time(dt);
        if let Err(e) = engine.vm.call("OnUpdate", vec![Value::Float(dt)]) {
            bevy::log::error!("[GROOT SCRIPT ERROR] entity #{}: {e}", comp.entity_id);
        }

        // Sync scratch -> ECS components (the source of truth), mirror the
        // result into ScriptTransform/ScriptColor for component readers, and
        // despawn entities that asked to die.
        CURRENT_STATE.with(|st| {
            let s = st.borrow();
            if s.destroy_requested {
                bevy::log::info!("[GROOT ECS] Despawning entity #{}", comp.entity_id);
                commands.entity(entity).despawn_recursive();
            } else {
                transform.translation.x = s.x;
                transform.translation.y = s.y;
                transform.rotation = Quat::from_rotation_z(s.rotation);
                transform.scale = Vec3::new(s.scale_x, s.scale_y, 1.0);
                if let Some(mut sprite) = sprite {
                    sprite.color =
                        Color::rgba(s.color.0, s.color.1, s.color.2, s.color.3);
                }

                script_tf.x = s.x;
                script_tf.y = s.y;
                script_tf.rotation = s.rotation;
                script_tf.scale_x = s.scale_x;
                script_tf.scale_y = s.scale_y;
                script_color.0 = Color::rgba(s.color.0, s.color.1, s.color.2, s.color.3);
            }
        });
    }

    // 3. Apply game-state writes queued by scripts to tagged ECS entities.
    GAME_WRITES.with(|writes| {
        for write in writes.borrow_mut().drain(..) {
            match write {
                GameWrite::PipePosition(idx, x, gap_y, gap_size) => {
                    let half_gap = gap_size / 2.0;
                    let sprite_height = 400.0;
                    for (mut transform, pipe_idx) in &mut pipe_query {
                        if pipe_idx.0 != idx {
                            continue;
                        }
                        transform.translation.x = x;
                        // Even index = top pipe (above the gap); odd = bottom.
                        transform.translation.y = if pipe_idx.0 % 2 == 0 {
                            gap_y + half_gap + sprite_height / 2.0
                        } else {
                            gap_y - half_gap - sprite_height / 2.0
                        };
                    }
                }
                GameWrite::ScoreDisplay(score, high) => {
                    for mut text in &mut score_query {
                        if let Some(section) = text.sections.first_mut() {
                            section.value = format!("Score: {score}  Best: {high}");
                        }
                    }
                }
            }
        }
    });
}

/// Process entity spawn requests queued by scripts.
fn handle_spawn_requests_system(
    mut commands: Commands,
    mut host: NonSendMut<GrootScriptHost>,
) {
    let mut reqs = match SPAWN_REQUESTS.lock() {
        Ok(reqs) => reqs,
        Err(_) => return,
    };
    for req in reqs.drain(..) {
        bevy::log::info!(
            "[GROOT ECS] Spawning '{}' at ({}, {})",
            req.script,
            req.x,
            req.y
        );
        // Ensure the engine is loaded for the new script.
        host.ensure_engine(&req.script);
        commands.spawn((
            SpriteBundle {
                sprite: Sprite {
                    color: Color::WHITE,
                    custom_size: Some(Vec2::new(32.0, 32.0)),
                    ..default()
                },
                transform: Transform::from_xyz(req.x, req.y, 1.0),
                ..default()
            },
            GoScriptComponent {
                script_path: req.script,
                entity_id: rand_entity_id(),
                tag: req.tag,
            },
            ScriptTransform {
                x: req.x,
                y: req.y,
                ..default()
            },
            ScriptColor::default(),
        ));
    }
}

/// Process gameplay events emitted by scripts.
fn handle_script_events_system() {
    let mut events = match SCRIPT_EVENTS.lock() {
        Ok(events) => events,
        Err(_) => return,
    };
    for ev in events.drain(..) {
        bevy::log::info!("[GROOT EVENT] '{}' payload={:.2}", ev.name, ev.payload);
    }
}

/// Render debug gizmo commands accumulated during the frame.
fn render_debug_gizmos_system(mut gizmos: Gizmos) {
    let mut cmds = match GIZMO_COMMANDS.lock() {
        Ok(cmds) => cmds,
        Err(_) => return,
    };
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

/// Stable pseudo-entity id for spawned script entities. Uses a global counter
/// so ids remain unique across the game session without depending on `rand`.
fn rand_entity_id() -> u32 {
    use std::sync::atomic::{AtomicU32, Ordering};
    static NEXT_ID: AtomicU32 = AtomicU32::new(10_000);
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}
