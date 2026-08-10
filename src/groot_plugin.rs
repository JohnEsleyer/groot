use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::rc::Rc;
use std::sync::Mutex;
use std::time::SystemTime;

use bevy::prelude::*;
use goscript::{HotReloadEngine, Value};
use serde::{Deserialize, Serialize};

use crate::groot_module::GrootModuleExt;

// ---------------------------------------------------------------------------
// Components & Resources
// ---------------------------------------------------------------------------

#[derive(Component, Clone, Debug)]
#[allow(dead_code)]
pub struct GoScriptComponent {
    pub script_path: String,
    pub entity_id: u32,
    pub tag: String,
}

#[derive(Component, Clone, Debug)]
pub struct SourcePrefab(pub String);

#[derive(Component, Clone, Copy, Debug, Deserialize, Serialize)]
pub enum Collider {
    None,
    Box2D { width: f32, height: f32 },
    Box3D { x: f32, y: f32, z: f32 },
    Sphere3D { radius: f32 },
}

impl Default for Collider {
    fn default() -> Self {
        Collider::None
    }
}

#[derive(Resource)]
pub struct DebugRender {
    pub show_colliders: bool,
}

impl Default for DebugRender {
    fn default() -> Self {
        Self {
            show_colliders: true,
        }
    }
}

#[derive(Component, Clone, Copy, Debug, Default)]
pub struct ScriptTransform {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub pitch: f32,
    pub yaw: f32,
    pub roll: f32,
    pub scale_x: f32,
    pub scale_y: f32,
    pub scale_z: f32,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct ScriptColor(pub Color);

impl Default for ScriptColor {
    fn default() -> Self {
        Self(Color::WHITE)
    }
}

#[derive(Clone, Debug)]
pub enum SpawnRequest {
    Prefab {
        path: String,
        x: f32,
        y: f32,
        z: f32,
        tag: String,
    },
    Script {
        script: String,
        x: f32,
        y: f32,
        z: f32,
        tag: String,
    },
}

#[derive(Clone, Debug)]
pub struct ScriptEvent {
    pub name: String,
    pub payload: f64,
}

// ---------------------------------------------------------------------------
// RON Configuration & Prefab Schemas
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Deserialize, Serialize, Resource)]
pub struct GrootConfig {
    pub project: ProjectConfig,
    pub window: WindowConfig,
    #[serde(default)]
    pub render: RenderConfig,
    pub initial_scene: String,
}

impl GrootConfig {
    pub fn load(path: &str) -> Self {
        let raw = match fs::read_to_string(path) {
            Ok(raw) => raw,
            Err(e) => {
                bevy::log::warn!("[GROOT CONFIG] missing '{path}' ({e}); using fallback");
                return Self::default();
            }
        };
        match ron::from_str(&raw) {
            Ok(config) => config,
            Err(e) => {
                bevy::log::warn!("[GROOT CONFIG] failed to parse '{path}': {e}; using fallback");
                Self::default()
            }
        }
    }
}

impl Default for GrootConfig {
    fn default() -> Self {
        Self {
            project: ProjectConfig::default(),
            window: WindowConfig::default(),
            render: RenderConfig::default(),
            initial_scene: "assets/scenes/main.scene.ron".into(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ProjectConfig {
    #[serde(default = "default_project_name")]
    pub name: String,
    #[serde(default = "default_version")]
    pub version: String,
}

fn default_project_name() -> String {
    "Groot Engine".into()
}
fn default_version() -> String {
    "0.2.0".into()
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            name: default_project_name(),
            version: default_version(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WindowConfig {
    #[serde(default = "default_title")]
    pub title: String,
    #[serde(default = "default_width")]
    pub width: f32,
    #[serde(default = "default_height")]
    pub height: f32,
}

fn default_title() -> String {
    "Groot".into()
}
fn default_width() -> f32 {
    1280.0
}
fn default_height() -> f32 {
    720.0
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            title: default_title(),
            width: default_width(),
            height: default_height(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RenderConfig {
    #[serde(default = "default_clear_color")]
    pub clear_color: RgbaColor,
}

fn default_clear_color() -> RgbaColor {
    RgbaColor::Rgba(0.1, 0.1, 0.12, 1.0)
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            clear_color: default_clear_color(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum RgbaColor {
    Rgba(f32, f32, f32, f32),
    Rgb(f32, f32, f32),
}

impl RgbaColor {
    pub fn to_color(&self) -> Color {
        match *self {
            RgbaColor::Rgba(r, g, b, a) => Color::rgba(r, g, b, a),
            RgbaColor::Rgb(r, g, b) => Color::rgb(r, g, b),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TransformConfig {
    #[serde(default)]
    pub position: (f32, f32, f32),
    #[serde(default)]
    pub rotation: (f32, f32, f32),
    #[serde(default = "default_scale")]
    pub scale: (f32, f32, f32),
}

fn default_scale() -> (f32, f32, f32) {
    (1.0, 1.0, 1.0)
}

impl Default for TransformConfig {
    fn default() -> Self {
        Self {
            position: (0.0, 0.0, 0.0),
            rotation: (0.0, 0.0, 0.0),
            scale: default_scale(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum VisualConfig {
    Sprite {
        size: (f32, f32),
        color: RgbaColor,
    },
    Text {
        value: String,
        size: f32,
        color: RgbaColor,
    },
    MeshPbr {
        shape: ShapeConfig,
        material: MaterialConfig,
    },
    Light(LightConfig),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum ShapeConfig {
    Cuboid { x: f32, y: f32, z: f32 },
    Sphere { radius: f32 },
    Cylinder { radius: f32, height: f32 },
    Plane { size_x: f32, size_z: f32 },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MaterialConfig {
    pub color: RgbaColor,
    #[serde(default = "default_roughness")]
    pub roughness: f32,
    #[serde(default = "default_metallic")]
    pub metallic: f32,
}

fn default_roughness() -> f32 {
    0.5
}
fn default_metallic() -> f32 {
    0.0
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum LightConfig {
    Point {
        color: RgbaColor,
        intensity: f32,
        range: f32,
        shadows_enabled: bool,
    },
    Directional {
        color: RgbaColor,
        illuminance: f32,
        shadows_enabled: bool,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PrefabConfig {
    pub name: String,
    #[serde(default)]
    pub script: Option<String>,
    #[serde(default)]
    pub transform: TransformConfig,
    #[serde(default)]
    pub visual: Option<VisualConfig>,
    #[serde(default)]
    pub collider: Option<Collider>,
    #[serde(default)]
    pub children: Vec<PrefabConfig>,
}

impl PrefabConfig {
    pub fn load(path: &str) -> Option<Self> {
        let raw = fs::read_to_string(path).ok()?;
        ron::from_str(&raw).ok()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, Default)]
pub struct SceneConfig {
    pub name: String,
    #[serde(default)]
    pub environment: EnvironmentConfig,
    #[serde(default)]
    pub entities: Vec<SceneEntityConfig>,
}

impl SceneConfig {
    pub fn load(path: &str) -> Self {
        let raw = match fs::read_to_string(path) {
            Ok(raw) => raw,
            Err(e) => {
                bevy::log::warn!("[GROOT SCENE] cannot read '{path}': {e}");
                return Self::default();
            }
        };
        ron::from_str(&raw).unwrap_or_default()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, Default)]
pub struct EnvironmentConfig {
    pub ambient_light: Option<AmbientLightConfig>,
    pub camera: Option<CameraConfig>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AmbientLightConfig {
    pub color: RgbaColor,
    pub brightness: f32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum CameraConfig {
    Perspective3D {
        fov: f32,
        position: (f32, f32, f32),
        look_at: (f32, f32, f32),
    },
    Orthographic2D,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SceneEntityConfig {
    pub prefab: String,
    #[serde(default)]
    pub entity_id: Option<u32>,
    #[serde(default)]
    pub tag: String,
    #[serde(default)]
    pub transform_override: Option<TransformOverride>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TransformOverride {
    pub position: Option<(f32, f32, f32)>,
    pub rotation: Option<(f32, f32, f32)>,
    pub scale: Option<(f32, f32, f32)>,
}

// ---------------------------------------------------------------------------
// Thread-Local State & Mutex Queues
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Default)]
struct EntityState {
    x: f32,
    y: f32,
    z: f32,
    pitch: f32,
    yaw: f32,
    roll: f32,
    scale_x: f32,
    scale_y: f32,
    scale_z: f32,
    color: (f32, f32, f32, f32),
    collider: Collider,
    destroy_requested: bool,
}

thread_local! {
    static CURRENT_ENTITY: Cell<u32> = const { Cell::new(0) };
    static CURRENT_STATE: RefCell<EntityState> = RefCell::new(EntityState::default());
    static ENTITY_POSITIONS: RefCell<HashMap<u32, (f32, f32, f32)>> = RefCell::new(HashMap::new());
}

static SPAWN_REQUESTS: Mutex<Vec<SpawnRequest>> = Mutex::new(Vec::new());
static SCRIPT_EVENTS: Mutex<Vec<ScriptEvent>> = Mutex::new(Vec::new());

// ---------------------------------------------------------------------------
// Shared Input & Script Host
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

pub struct GrootScriptHost {
    engines: HashMap<String, HotReloadEngine>,
    input: Rc<InputState>,
    mtime_cache: HashMap<String, SystemTime>,
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
            mtime_cache: HashMap::new(),
        }
    }

    fn ensure_engine(&mut self, script_path: &str) -> &mut HotReloadEngine {
        self.engines
            .entry(script_path.to_string())
            .or_insert_with(|| {
                let mut engine = HotReloadEngine::new(script_path);
                let vm = &mut engine.vm;

                vm.register_groot_module();

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
                vm.register_fn("groot.GetMouseWorld", move |_| {
                    let (mx, my) = inp_mouse.mouse_pos.get();
                    Value::Slice(Rc::new(RefCell::new(vec![
                        Value::Float(mx),
                        Value::Float(my),
                    ])))
                });

                let inp_mdown = Rc::clone(&self.input);
                vm.register_fn("groot.IsMouseDown", move |args| {
                    let btn = args.first().and_then(|v| v.as_number()).unwrap_or(0.0) as usize;
                    Value::Bool(inp_mdown.mouse_button_down.get()[btn.min(2)])
                });

                let inp_mpressed = Rc::clone(&self.input);
                vm.register_fn("groot.IsMousePressed", move |args| {
                    let btn = args.first().and_then(|v| v.as_number()).unwrap_or(0.0) as usize;
                    Value::Bool(inp_mpressed.mouse_button_pressed.get()[btn.min(2)])
                });

                vm.register_fn("groot.GetSelfEntity", |_| {
                    let id = CURRENT_ENTITY.with(|c| c.get());
                    Value::Int(id as i64)
                });

                vm.register_fn("groot.GetSelfPosition", |_| {
                    let s = CURRENT_STATE.with(|st| *st.borrow());
                    Value::Slice(Rc::new(RefCell::new(vec![
                        Value::Float(s.x as f64),
                        Value::Float(s.y as f64),
                        Value::Float(s.z as f64),
                    ])))
                });

                vm.register_fn("groot.SetSelfPosition", |args| {
                    let nx = args.first().and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
                    let ny = args.get(1).and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
                    let nz = args.get(2).and_then(|v| v.as_number());
                    CURRENT_STATE.with(|st| {
                        let mut s = st.borrow_mut();
                        s.x = nx;
                        s.y = ny;
                        if let Some(z) = nz {
                            s.z = z as f32;
                        }
                    });
                    Value::Nil
                });

                vm.register_fn("groot.GetSelfRotation", |_| {
                    let s = CURRENT_STATE.with(|st| *st.borrow());
                    Value::Float(s.yaw as f64)
                });

                vm.register_fn("groot.SetSelfRotation", |args| {
                    let rot = args.first().and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
                    CURRENT_STATE.with(|st| st.borrow_mut().yaw = rot);
                    Value::Nil
                });

                vm.register_fn("groot.SetSelfRotation3D", |args| {
                    let p = args.first().and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
                    let y = args.get(1).and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
                    let r = args.get(2).and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
                    CURRENT_STATE.with(|st| {
                        let mut s = st.borrow_mut();
                        s.pitch = p;
                        s.yaw = y;
                        s.roll = r;
                    });
                    Value::Nil
                });

                vm.register_fn("groot.GetSelfScale", |_| {
                    let s = CURRENT_STATE.with(|st| *st.borrow());
                    Value::Slice(Rc::new(RefCell::new(vec![
                        Value::Float(s.scale_x as f64),
                        Value::Float(s.scale_y as f64),
                        Value::Float(s.scale_z as f64),
                    ])))
                });

                vm.register_fn("groot.SetSelfScale", |args| {
                    let sx = args.first().and_then(|v| v.as_number()).unwrap_or(1.0) as f32;
                    let sy = args.get(1).and_then(|v| v.as_number()).unwrap_or(1.0) as f32;
                    let sz = args.get(2).and_then(|v| v.as_number()).unwrap_or(1.0) as f32;
                    CURRENT_STATE.with(|st| {
                        let mut s = st.borrow_mut();
                        s.scale_x = sx;
                        s.scale_y = sy;
                        s.scale_z = sz;
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

                vm.register_fn("groot.SetSelfMaterialColor", |args| {
                    let r = args.first().and_then(|v| v.as_number()).unwrap_or(1.0) as f32;
                    let g = args.get(1).and_then(|v| v.as_number()).unwrap_or(1.0) as f32;
                    let b = args.get(2).and_then(|v| v.as_number()).unwrap_or(1.0) as f32;
                    let a = args.get(3).and_then(|v| v.as_number()).unwrap_or(1.0) as f32;
                    CURRENT_STATE.with(|st| st.borrow_mut().color = (r, g, b, a));
                    Value::Nil
                });

                vm.register_fn("groot.SetSelfCollider", |args| {
                    let w = args.first().and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
                    let h = args.get(1).and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
                    let z = args.get(2).and_then(|v| v.as_number());
                    CURRENT_STATE.with(|st| {
                        let mut s = st.borrow_mut();
                        s.collider = match z {
                            Some(d) => Collider::Box3D { x: w, y: h, z: d as f32 },
                            None => Collider::Box2D { width: w, height: h },
                        };
                    });
                    Value::Nil
                });

                vm.register_fn("groot.DestroySelf", |_| {
                    CURRENT_STATE.with(|st| st.borrow_mut().destroy_requested = true);
                    Value::Nil
                });

                vm.register_fn("groot.GetEntityPosition", |args| {
                    let tid = args.first().and_then(|v| v.as_number()).unwrap_or(0.0) as u32;
                    let pos = ENTITY_POSITIONS
                        .with(|p| p.borrow().get(&tid).copied())
                        .unwrap_or((0.0, 0.0, 0.0));
                    Value::Slice(Rc::new(RefCell::new(vec![
                        Value::Float(pos.0 as f64),
                        Value::Float(pos.1 as f64),
                        Value::Float(pos.2 as f64),
                    ])))
                });

                vm.register_fn("groot.GetDistance", |args| {
                    let id1 = args.first().and_then(|v| v.as_number()).unwrap_or(0.0) as u32;
                    let id2 = args.get(1).and_then(|v| v.as_number()).unwrap_or(0.0) as u32;
                    let (p1, p2) = ENTITY_POSITIONS.with(|p| {
                        let map = p.borrow();
                        (
                            map.get(&id1).copied().unwrap_or((0.0, 0.0, 0.0)),
                            map.get(&id2).copied().unwrap_or((0.0, 0.0, 0.0)),
                        )
                    });
                    let dx = p2.0 - p1.0;
                    let dy = p2.1 - p1.1;
                    let dz = p2.2 - p1.2;
                    Value::Float(((dx * dx + dy * dy + dz * dz) as f64).sqrt())
                });

                vm.register_fn("groot.SpawnEntity", |args| {
                    let script = args.first().and_then(|v| v.as_string()).unwrap_or("").to_string();
                    let x = args.get(1).and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
                    let y = args.get(2).and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
                    let z = args.get(3).and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
                    let tag = args.get(4).and_then(|v| v.as_string()).unwrap_or("").to_string();
                    if let Ok(mut reqs) = SPAWN_REQUESTS.lock() {
                        reqs.push(SpawnRequest::Script { script, x, y, z, tag });
                    }
                    Value::Nil
                });

                vm.register_fn("groot.SpawnPrefab", |args| {
                    let path = args.first().and_then(|v| v.as_string()).unwrap_or("").to_string();
                    let x = args.get(1).and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
                    let y = args.get(2).and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
                    let z = args.get(3).and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
                    let tag = args.get(4).and_then(|v| v.as_string()).unwrap_or("").to_string();
                    if let Ok(mut reqs) = SPAWN_REQUESTS.lock() {
                        reqs.push(SpawnRequest::Prefab { path, x, y, z, tag });
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
            .init_resource::<DebugRender>()
            .add_systems(Startup, spawn_scene_system)
            .add_systems(
                Update,
                (
                    script_hot_reload_system,
                    ron_hot_reload_system,
                    script_input_sync_system,
                    script_execution_system,
                    handle_spawn_requests_system,
                    handle_script_events_system,
                    render_collider_debug_system,
                )
                    .chain(),
            );
    }
}

// ---------------------------------------------------------------------------
// Systems
// ---------------------------------------------------------------------------

fn script_hot_reload_system(mut host: NonSendMut<GrootScriptHost>) {
    host.reload_all();
}

fn ron_hot_reload_system(
    mut host: NonSendMut<GrootScriptHost>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    query: Query<(Entity, &SourcePrefab)>,
) {
    for (entity, source) in &query {
        let path = Path::new(&source.0);
        if !path.exists() {
            continue;
        }
        if let Ok(meta) = fs::metadata(path) {
            if let Ok(mtime) = meta.modified() {
                let cached = host.mtime_cache.get(&source.0).copied();
                if cached.map_or(true, |c| mtime > c) {
                    host.mtime_cache.insert(source.0.clone(), mtime);
                    if let Some(prefab) = PrefabConfig::load(&source.0) {
                        bevy::log::info!("[GROOT RON] Live-reloading prefab asset '{}'", source.0);
                        commands.entity(entity).despawn_descendants();
                        update_entity_visuals(&mut commands, entity, &prefab, &mut meshes, &mut materials);
                    }
                }
            }
        }
    }
}

fn script_input_sync_system(
    host: NonSendMut<GrootScriptHost>,
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mouse_input: Res<ButtonInput<MouseButton>>,
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

type ScriptEntityQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static GoScriptComponent,
        &'static mut Transform,
        Option<&'static mut Sprite>,
        Option<&'static Handle<StandardMaterial>>,
        &'static mut ScriptTransform,
        &'static mut ScriptColor,
        Option<&'static mut Collider>,
    ),
>;

fn script_execution_system(
    mut host: NonSendMut<GrootScriptHost>,
    time: Res<Time>,
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut script_query: ScriptEntityQuery,
) {
    let dt = time.delta_seconds() as f64;

    ENTITY_POSITIONS.with(|snapshot| {
        let mut map = snapshot.borrow_mut();
        map.clear();
        for (_, comp, transform, ..) in &script_query {
            map.insert(
                comp.entity_id,
                (
                    transform.translation.x,
                    transform.translation.y,
                    transform.translation.z,
                ),
            );
        }
    });

    for (entity, comp, mut transform, sprite, mat_handle, mut script_tf, mut script_color, collider) in
        &mut script_query
    {
        CURRENT_ENTITY.with(|c| c.set(comp.entity_id));

        CURRENT_STATE.with(|st| {
            let mut s = st.borrow_mut();
            s.x = transform.translation.x;
            s.y = transform.translation.y;
            s.z = transform.translation.z;
            let (p, y, r) = transform.rotation.to_euler(EulerRot::YXZ);
            s.pitch = p.to_degrees();
            s.yaw = y.to_degrees();
            s.roll = r.to_degrees();
            s.scale_x = transform.scale.x;
            s.scale_y = transform.scale.y;
            s.scale_z = transform.scale.z;
            if let Some(sprite) = &sprite {
                let c = sprite.color;
                s.color = (c.r(), c.g(), c.b(), c.a());
            } else if let Some(mat_h) = mat_handle {
                if let Some(mat) = materials.get(mat_h) {
                    let c = mat.base_color;
                    s.color = (c.r(), c.g(), c.b(), c.a());
                }
            }
            if let Some(collider) = &collider {
                s.collider = **collider;
            }
            s.destroy_requested = false;
        });

        let engine = host.ensure_engine(&comp.script_path);
        engine.vm.set_delta_time(dt);
        if let Err(e) = engine.vm.call("OnUpdate", vec![Value::Float(dt)]) {
            bevy::log::error!("[GROOT SCRIPT ERROR] entity #{}: {e}", comp.entity_id);
        }

        CURRENT_STATE.with(|st| {
            let s = st.borrow();
            if s.destroy_requested {
                bevy::log::info!("[GROOT ECS] Despawning entity #{}", comp.entity_id);
                commands.entity(entity).despawn_recursive();
            } else {
                transform.translation.x = s.x;
                transform.translation.y = s.y;
                transform.translation.z = s.z;
                transform.rotation = Quat::from_euler(
                    EulerRot::YXZ,
                    s.yaw.to_radians(),
                    s.pitch.to_radians(),
                    s.roll.to_radians(),
                );
                transform.scale = Vec3::new(s.scale_x, s.scale_y, s.scale_z);

                let color = Color::rgba(s.color.0, s.color.1, s.color.2, s.color.3);
                if let Some(mut sprite) = sprite {
                    sprite.color = color;
                }
                if let Some(mat_h) = mat_handle {
                    if let Some(mat) = materials.get_mut(mat_h) {
                        mat.base_color = color;
                    }
                }

                script_tf.x = s.x;
                script_tf.y = s.y;
                script_tf.z = s.z;
                script_tf.pitch = s.pitch;
                script_tf.yaw = s.yaw;
                script_tf.roll = s.roll;
                script_tf.scale_x = s.scale_x;
                script_tf.scale_y = s.scale_y;
                script_tf.scale_z = s.scale_z;
                script_color.0 = color;
                if let Some(mut collider) = collider {
                    *collider = s.collider;
                }
            }
        });
    }
}

fn spawn_scene_system(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    config: Option<Res<GrootConfig>>,
) {
    let Some(config) = config else { return };
    let scene = SceneConfig::load(&config.initial_scene);
    bevy::log::info!(
        "[GROOT] Spawning scene '{}' ({} entities)",
        scene.name,
        scene.entities.len()
    );

    if let Some(ambient) = &scene.environment.ambient_light {
        commands.insert_resource(AmbientLight {
            color: ambient.color.to_color(),
            brightness: ambient.brightness,
        });
    }

    if let Some(camera) = &scene.environment.camera {
        match camera {
            CameraConfig::Perspective3D { fov, position, look_at } => {
                let pos = Vec3::new(position.0, position.1, position.2);
                let target = Vec3::new(look_at.0, look_at.1, look_at.2);
                commands.spawn(Camera3dBundle {
                    transform: Transform::from_translation(pos).looking_at(target, Vec3::Y),
                    projection: PerspectiveProjection {
                        fov: fov.to_radians(),
                        ..default()
                    }
                    .into(),
                    ..default()
                });
            }
            CameraConfig::Orthographic2D => {
                commands.spawn(Camera2dBundle::default());
            }
        }
    } else {
        commands.spawn(Camera3dBundle {
            transform: Transform::from_xyz(0.0, 8.0, 14.0).looking_at(Vec3::ZERO, Vec3::Y),
            ..default()
        });
    }

    for entity_cfg in &scene.entities {
        if let Some(prefab) = PrefabConfig::load(&entity_cfg.prefab) {
            spawn_prefab_tree(
                &mut commands,
                &mut meshes,
                &mut materials,
                &prefab,
                &entity_cfg.prefab,
                entity_cfg.entity_id,
                &entity_cfg.tag,
                entity_cfg.transform_override.as_ref(),
            );
        } else {
            bevy::log::warn!("[GROOT SCENE] Missing prefab asset '{}'", entity_cfg.prefab);
        }
    }
}

fn spawn_prefab_tree(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    prefab: &PrefabConfig,
    prefab_path: &str,
    override_id: Option<u32>,
    tag: &str,
    tf_override: Option<&TransformOverride>,
) -> Entity {
    let mut pos = Vec3::new(
        prefab.transform.position.0,
        prefab.transform.position.1,
        prefab.transform.position.2,
    );
    let mut rot = Vec3::new(
        prefab.transform.rotation.0,
        prefab.transform.rotation.1,
        prefab.transform.rotation.2,
    );
    let mut scale = Vec3::new(
        prefab.transform.scale.0,
        prefab.transform.scale.1,
        prefab.transform.scale.2,
    );

    if let Some(ov) = tf_override {
        if let Some(p) = ov.position {
            pos = Vec3::new(p.0, p.1, p.2);
        }
        if let Some(r) = ov.rotation {
            rot = Vec3::new(r.0, r.1, r.2);
        }
        if let Some(s) = ov.scale {
            scale = Vec3::new(s.0, s.1, s.2);
        }
    }

    let transform = Transform {
        translation: pos,
        rotation: Quat::from_euler(
            EulerRot::YXZ,
            rot.y.to_radians(),
            rot.x.to_radians(),
            rot.z.to_radians(),
        ),
        scale,
    };

    let id = override_id.unwrap_or_else(rand_entity_id);
    let collider = prefab.collider.unwrap_or_default();

    let mut entity_builder = commands.spawn((
        SpatialBundle {
            transform,
            ..default()
        },
        SourcePrefab(prefab_path.to_string()),
        ScriptTransform {
            x: pos.x,
            y: pos.y,
            z: pos.z,
            pitch: rot.x,
            yaw: rot.y,
            roll: rot.z,
            scale_x: scale.x,
            scale_y: scale.y,
            scale_z: scale.z,
        },
        ScriptColor::default(),
        collider,
    ));

    if let Some(script) = &prefab.script {
        entity_builder.insert(GoScriptComponent {
            script_path: script.clone(),
            entity_id: id,
            tag: tag.to_string(),
        });
    }

    let parent_entity = entity_builder.id();

    attach_visual_components(commands, parent_entity, prefab, meshes, materials);

    for child_cfg in &prefab.children {
        let child_entity = spawn_prefab_tree(
            commands,
            meshes,
            materials,
            child_cfg,
            "",
            None,
            "",
            None,
        );
        commands.entity(parent_entity).add_child(child_entity);
    }

    parent_entity
}

fn update_entity_visuals(
    commands: &mut Commands,
    entity: Entity,
    prefab: &PrefabConfig,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
) {
    attach_visual_components(commands, entity, prefab, meshes, materials);
}

fn attach_visual_components(
    commands: &mut Commands,
    entity: Entity,
    prefab: &PrefabConfig,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
) {
    let Some(visual) = &prefab.visual else { return };
    match visual {
        VisualConfig::Sprite { size, color } => {
            commands.entity(entity).insert((
                Sprite {
                    color: color.to_color(),
                    custom_size: Some(Vec2::new(size.0, size.1)),
                    ..default()
                },
                ScriptColor(color.to_color()),
            ));
        }
        VisualConfig::Text { value, size, color } => {
            commands.entity(entity).insert(Text2dBundle {
                text: Text::from_section(
                    value,
                    TextStyle {
                        font_size: *size,
                        color: color.to_color(),
                        ..default()
                    },
                ),
                ..default()
            });
        }
        VisualConfig::MeshPbr { shape, material } => {
            let mesh_handle = match shape {
                ShapeConfig::Cuboid { x, y, z } => meshes.add(Cuboid::new(*x, *y, *z)),
                ShapeConfig::Sphere { radius } => meshes.add(Sphere::new(*radius)),
                ShapeConfig::Cylinder { radius, height } => meshes.add(Cylinder::new(*radius, *height)),
                ShapeConfig::Plane { size_x, size_z } => meshes.add(Plane3d::default().mesh().size(*size_x, *size_z)),
            };
            let mat_handle = materials.add(StandardMaterial {
                base_color: material.color.to_color(),
                perceptual_roughness: material.roughness,
                metallic: material.metallic,
                ..default()
            });
            commands.entity(entity).insert((
                mesh_handle,
                mat_handle,
                ScriptColor(material.color.to_color()),
            ));
        }
        VisualConfig::Light(light) => match light {
            LightConfig::Point {
                color,
                intensity,
                range,
                shadows_enabled,
            } => {
                commands.entity(entity).insert(PointLight {
                    color: color.to_color(),
                    intensity: *intensity,
                    range: *range,
                    shadows_enabled: *shadows_enabled,
                    ..default()
                });
            }
            LightConfig::Directional {
                color,
                illuminance,
                shadows_enabled,
            } => {
                commands.entity(entity).insert(DirectionalLight {
                    color: color.to_color(),
                    illuminance: *illuminance,
                    shadows_enabled: *shadows_enabled,
                    ..default()
                });
            }
        },
    }
}

fn handle_spawn_requests_system(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut host: NonSendMut<GrootScriptHost>,
) {
    let mut reqs = match SPAWN_REQUESTS.lock() {
        Ok(reqs) => reqs,
        Err(_) => return,
    };
    for req in reqs.drain(..) {
        match req {
            SpawnRequest::Prefab { path, x, y, z, tag } => {
                if let Some(prefab) = PrefabConfig::load(&path) {
                    if let Some(script) = &prefab.script {
                        host.ensure_engine(script);
                    }
                    let ov = TransformOverride {
                        position: Some((x, y, z)),
                        rotation: None,
                        scale: None,
                    };
                    spawn_prefab_tree(
                        &mut commands,
                        &mut meshes,
                        &mut materials,
                        &prefab,
                        &path,
                        None,
                        &tag,
                        Some(&ov),
                    );
                }
            }
            SpawnRequest::Script { script, x, y, z, tag } => {
                host.ensure_engine(&script);
                commands.spawn((
                    SpatialBundle {
                        transform: Transform::from_xyz(x, y, z),
                        ..default()
                    },
                    GoScriptComponent {
                        script_path: script,
                        entity_id: rand_entity_id(),
                        tag,
                    },
                    ScriptTransform {
                        x,
                        y,
                        z,
                        ..default()
                    },
                    ScriptColor::default(),
                    Collider::default(),
                ));
            }
        }
    }
}

fn handle_script_events_system() {
    let mut events = match SCRIPT_EVENTS.lock() {
        Ok(events) => events,
        Err(_) => return,
    };
    for ev in events.drain(..) {
        bevy::log::info!("[GROOT EVENT] '{}' payload={:.2}", ev.name, ev.payload);
    }
}

fn render_collider_debug_system(
    mut gizmos: Gizmos,
    debug: Res<DebugRender>,
    query: Query<(&Transform, &Collider)>,
) {
    if !debug.show_colliders {
        return;
    }
    let color = Color::rgba(0.3, 1.0, 0.4, 1.0);
    for (transform, collider) in &query {
        match collider {
            Collider::Box2D { width, height } => {
                let (_, _, rot) = transform.rotation.to_euler(EulerRot::XYZ);
                gizmos.rect_2d(
                    transform.translation.truncate(),
                    rot,
                    Vec2::new(*width, *height),
                    color,
                );
            }
            Collider::Box3D { x: _, y: _, z: _ } => {
                gizmos.cuboid(*transform, color);
            }
            Collider::Sphere3D { radius } => {
                gizmos.sphere(transform.translation, transform.rotation, *radius, color);
            }
            Collider::None => {}
        }
    }
}

fn rand_entity_id() -> u32 {
    use std::sync::atomic::{AtomicU32, Ordering};
    static NEXT_ID: AtomicU32 = AtomicU32::new(10_000);
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}
