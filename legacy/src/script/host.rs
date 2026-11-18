use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Mutex;

use goscript::value::Value;
use goscript::vm::VirtualMachine;
use goscript::HotReloadEngine;
use hecs::World;

use crate::assets::ron_loader::{PrefabConfig, ShapeConfig, VisualConfig};
use crate::ecs::*;
use crate::groot_module::GrootModuleExt;
use crate::script::input::InputState;

#[derive(Debug, Clone, Default)]
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
    text_value: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct CameraCommandState {
    pub pos: (f32, f32, f32),
    pub target: (f32, f32, f32),
    pub fov: f32,
    pub viewport: (f32, f32),
    pub modified_3d: bool,
    pub modified_2d: bool,
}

thread_local! {
    static CURRENT_ENTITY: Cell<u32> = const { Cell::new(0) };
    static CURRENT_STATE: RefCell<EntityState> = RefCell::new(EntityState::default());
    static ENTITY_POSITIONS: RefCell<HashMap<u32, (f32, f32, f32)>> = RefCell::new(HashMap::new());
    static TAG_POSITIONS: RefCell<HashMap<String, Vec<(f32, f32, f32)>>> = RefCell::new(HashMap::new());
    static CAMERA_COMMANDS: RefCell<CameraCommandState> = RefCell::new(CameraCommandState {
        pos: (0.0, 0.0, 10.0),
        target: (0.0, 0.0, 0.0),
        fov: 60.0,
        viewport: (21.33, 12.0),
        modified_3d: false,
        modified_2d: false,
    });
}

#[derive(Clone, Debug)]
pub enum SpawnRequest {
    Prefab { path: String, x: f32, y: f32, z: f32, tag: String },
}

static SPAWN_REQUESTS: Mutex<Vec<SpawnRequest>> = Mutex::new(Vec::new());
static DESPAWN_TAGS: Mutex<Vec<String>> = Mutex::new(Vec::new());
static NEXT_ENTITY_ID: Mutex<u32> = Mutex::new(20000);
static SET_TEXT_BY_TAG_REQUESTS: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());

/// Cached metadata for a loaded script — tracks which lifecycle hooks are defined.
#[derive(Debug, Clone, Default)]
pub struct ScriptMetadata {
    pub has_on_update: bool,
    pub has_on_start: bool,
}

pub struct LoadedScript {
    pub engine: HotReloadEngine,
    pub metadata: ScriptMetadata,
    /// One VM instance per entity running this script.
    ///
    /// Every instance owns its own global table, so a script's top-level
    /// state (`var self = PipeController{...}`) is private to the entity
    /// instead of being shared by every entity that runs the same file.
    pub instances: HashMap<u32, VirtualMachine>,
}

pub struct GrootScriptHost {
    scripts: HashMap<String, LoadedScript>,
    pub input: Rc<InputState>,
}

impl GrootScriptHost {
    pub fn new() -> Self {
        Self {
            scripts: HashMap::new(),
            input: Rc::new(InputState::new()),
        }
    }

    pub fn ensure_engine(&mut self, script_path: &str) -> &mut LoadedScript {
        let input_ref = Rc::clone(&self.input);

        self.scripts
            .entry(script_path.to_string())
            .or_insert_with(|| {
                // ------------------------------------------------------------------
                // Build the engine.  In debug desktop builds we use the file-based
                // HotReloadEngine so live edits are picked up automatically.  In
                // release (and on Android) we compile directly from the decrypted
                // in-memory source – no temp file is ever written.
                // ------------------------------------------------------------------
                let mut engine = {
                    #[cfg(all(debug_assertions, not(target_os = "android")))]
                    {
                        let resolved = crate::assets::prepare_script_path(script_path);
                        HotReloadEngine::new(&resolved)
                    }
                    #[cfg(not(all(debug_assertions, not(target_os = "android"))))]
                    {
                        // Load + decrypt + minify entirely in memory.
                        let source = crate::assets::load_script_source(script_path)
                            .unwrap_or_default();
                        HotReloadEngine::from_str(script_path, &source)
                    }
                };

                register_host_api(&mut engine.vm, &input_ref);

                // In debug mode, trigger the initial file-based compile.
                #[cfg(all(debug_assertions, not(target_os = "android")))]
                let _ = engine.reload_if_changed();

                let metadata = ScriptMetadata {
                    has_on_update: engine.vm.globals.contains_key("OnUpdate"),
                    has_on_start: engine.vm.globals.contains_key("OnStart"),
                };

                LoadedScript {
                    engine,
                    metadata,
                    instances: HashMap::new(),
                }
            })
    }

    /// Returns the VM instance that runs `script_path` for `entity_id`,
    /// creating it on first use.
    ///
    /// The instance is seeded with a deep copy of the script's globals, so
    /// every entity owns its own top-level state. Two entities running the
    /// same script therefore never observe each other's mutations.
    pub fn ensure_instance(&mut self, script_path: &str, entity_id: u32) -> &mut VirtualMachine {
        // Make sure the script is loaded and compiled.
        self.ensure_engine(script_path);

        let loaded = self
            .scripts
            .get_mut(script_path)
            .expect("script was just ensured");

        if !loaded.instances.contains_key(&entity_id) {
            let mut vm = VirtualMachine::new();
            register_host_api(&mut vm, &self.input);
            // Seed the instance with the script's globals. Mutable values are
            // deep-copied, so `var self = PipeController{...}` becomes one
            // private struct per entity instead of one shared struct.
            for (name, value) in loaded.engine.vm.globals.iter() {
                vm.globals.insert(name.clone(), value.deep_clone());
            }
            loaded.instances.insert(entity_id, vm);
        }

        loaded
            .instances
            .get_mut(&entity_id)
            .expect("instance was just created")
    }

    /// Returns the VM instance of an entity, if it exists.
    ///
    /// Useful for engine tooling and tests that need to inspect an entity's
    /// live script state.
    pub fn instance(&self, script_path: &str, entity_id: u32) -> Option<&VirtualMachine> {
        self.scripts
            .get(script_path)
            .and_then(|loaded| loaded.instances.get(&entity_id))
    }

    /// Releases the VM instances of entities that no longer exist.
    pub fn prune_dead_entities(&mut self, live: &HashSet<u32>) {
        for loaded in self.scripts.values_mut() {
            loaded.instances.retain(|id, _| live.contains(id));
        }
    }

    pub fn reload_all(&mut self) {
        // Hot-reload is only meaningful in debug desktop builds where scripts
        // are read from disk.  In release/Android the source is baked in and
        // cannot change at runtime.
        #[cfg(all(debug_assertions, not(target_os = "android")))]
        for loaded in self.scripts.values_mut() {
            if let Ok(reloaded) = loaded.engine.reload_if_changed() {
                if reloaded {
                    loaded.metadata.has_on_update = loaded.engine.vm.globals.contains_key("OnUpdate");
                    loaded.metadata.has_on_start = loaded.engine.vm.globals.contains_key("OnStart");
                    // Hot-swap the new function bodies into every live entity
                    // instance; each instance keeps its own global state.
                    sync_instances(loaded);
                    log::info!(
                        "[GROOT SCRIPT] Reloaded with hooks: update={}, start={}",
                        loaded.metadata.has_on_update,
                        loaded.metadata.has_on_start
                    );
                }
            }
        }
    }
}

/// Hot-swaps a freshly compiled script into every live entity instance.
///
/// Functions are replaced with the new bytecode. Every other global keeps the
/// instance's own value (that is the whole point of hot reloading), and
/// globals the script gained since the last reload are seeded from the
/// template with a deep copy.
fn sync_instances(loaded: &mut LoadedScript) {
    for instance in loaded.instances.values_mut() {
        for (name, value) in loaded.engine.vm.globals.iter() {
            match value {
                Value::Function(_) => {
                    instance.globals.insert(name.clone(), value.clone());
                }
                _ => {
                    instance
                        .globals
                        .entry(name.clone())
                        .or_insert_with(|| value.deep_clone());
                }
            }
        }
    }
}

/// Registers the whole `groot.*` scripting API on a VM. Every VM that runs a
/// script — the compiled template as well as each per-entity instance — needs
/// the full API surface.
fn register_host_api(vm: &mut VirtualMachine, input: &Rc<InputState>) {
    vm.register_groot_module();

    let inp_axis = Rc::clone(input);
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

    let inp_key = Rc::clone(input);
    vm.register_fn("groot.IsKeyDown", move |args| {
        if let Some(key) = args.first().and_then(|v| v.as_string()) {
            return Value::Bool(inp_key.keys_down.borrow().iter().any(|k| k == key));
        }
        Value::Bool(false)
    });

    let inp_pressed = Rc::clone(input);
    vm.register_fn("groot.IsKeyPressed", move |args| {
        if let Some(key) = args.first().and_then(|v| v.as_string()) {
            return Value::Bool(inp_pressed.keys_just_pressed.borrow().iter().any(|k| k == key));
        }
        Value::Bool(false)
    });

    let inp_mouse_down = Rc::clone(input);
    vm.register_fn("groot.IsMouseDown", move |args| {
        let btn = args.first().and_then(|v| v.as_number()).unwrap_or(0.0) as usize;
        if btn < 3 {
            return Value::Bool(inp_mouse_down.mouse_button_down.get()[btn]);
        }
        Value::Bool(false)
    });

    let inp_mouse_pressed = Rc::clone(input);
    vm.register_fn("groot.IsMousePressed", move |args| {
        let btn = args.first().and_then(|v| v.as_number()).unwrap_or(0.0) as usize;
        if btn < 3 {
            return Value::Bool(inp_mouse_pressed.mouse_button_pressed.get()[btn]);
        }
        Value::Bool(false)
    });

    vm.register_fn("groot.GetSelfEntity", |_| {
        Value::Int(CURRENT_ENTITY.with(|c| c.get()) as i64)
    });

    vm.register_fn("groot.GetSelfPosition", |_| {
        let s = CURRENT_STATE.with(|st| st.borrow().clone());
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
        CURRENT_STATE.with(|st| {
            st.borrow_mut().color = (r, g, b, a);
        });
        Value::Nil
    });

    vm.register_fn("groot.GetEntityPosition", |args| {
        let id = args.first().and_then(|v| v.as_number()).unwrap_or(0.0) as u32;
        let pos = ENTITY_POSITIONS.with(|map| map.borrow().get(&id).copied());
        if let Some((x, y, z)) = pos {
            Value::Slice(Rc::new(RefCell::new(vec![
                Value::Float(x as f64),
                Value::Float(y as f64),
                Value::Float(z as f64),
            ])))
        } else {
            Value::Slice(Rc::new(RefCell::new(vec![
                Value::Float(0.0),
                Value::Float(0.0),
                Value::Float(0.0),
            ])))
        }
    });

    vm.register_fn("groot.GetTagPositions", |args| {
        let tag = args.first().and_then(|v| v.as_string()).unwrap_or("").to_string();
        let positions = TAG_POSITIONS.with(|map| map.borrow().get(&tag).cloned());
        let values: Vec<Value> = positions
            .unwrap_or_default()
            .into_iter()
            .map(|(x, y, z)| {
                Value::Slice(Rc::new(RefCell::new(vec![
                    Value::Float(x as f64),
                    Value::Float(y as f64),
                    Value::Float(z as f64),
                ])))
            })
            .collect();
        Value::Slice(Rc::new(RefCell::new(values)))
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

    vm.register_fn("groot.SetCameraPosition", |args| {
        let x = args.first().and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
        let y = args.get(1).and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
        let z = args.get(2).and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
        CAMERA_COMMANDS.with(|c| {
            let mut cam = c.borrow_mut();
            cam.pos = (x, y, z);
            cam.modified_3d = true;
            cam.modified_2d = true;
        });
        Value::Nil
    });

    vm.register_fn("groot.GetCameraPosition", |_| {
        let pos = CAMERA_COMMANDS.with(|c| c.borrow().pos);
        Value::Slice(Rc::new(RefCell::new(vec![
            Value::Float(pos.0 as f64),
            Value::Float(pos.1 as f64),
            Value::Float(pos.2 as f64),
        ])))
    });

    vm.register_fn("groot.SetCameraTarget", |args| {
        let x = args.first().and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
        let y = args.get(1).and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
        let z = args.get(2).and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
        CAMERA_COMMANDS.with(|c| {
            let mut cam = c.borrow_mut();
            cam.target = (x, y, z);
            cam.modified_3d = true;
        });
        Value::Nil
    });

    vm.register_fn("groot.SetCameraFov", |args| {
        let fov = args.first().and_then(|v| v.as_number()).unwrap_or(60.0) as f32;
        CAMERA_COMMANDS.with(|c| {
            let mut cam = c.borrow_mut();
            cam.fov = fov;
            cam.modified_3d = true;
        });
        Value::Nil
    });

    vm.register_fn("groot.SetCameraViewport", |args| {
        let w = args.first().and_then(|v| v.as_number()).unwrap_or(21.33) as f32;
        let h = args.get(1).and_then(|v| v.as_number()).unwrap_or(12.0) as f32;
        CAMERA_COMMANDS.with(|c| {
            let mut cam = c.borrow_mut();
            cam.viewport = (w, h);
            cam.modified_2d = true;
        });
        Value::Nil
    });

    vm.register_fn("groot.SetSelfText", |args| {
        if let Some(text) = args.first().and_then(|v| v.as_string()) {
            CURRENT_STATE.with(|st| {
                st.borrow_mut().text_value = Some(text.to_string());
            });
        }
        Value::Nil
    });

    vm.register_fn("groot.SetTextByTag", |args| {
        let tag = args.first().and_then(|v| v.as_string()).unwrap_or_default();
        let text = args.get(1).and_then(|v| v.as_string()).unwrap_or_default();
        if let Ok(mut reqs) = SET_TEXT_BY_TAG_REQUESTS.lock() {
            reqs.push((tag.to_string(), text.to_string()));
        }
        Value::Nil
    });

    vm.register_fn("groot.DespawnByTag", |args| {
        let tag = args.first().and_then(|v| v.as_string()).unwrap_or("").to_string();
        if let Ok(mut tags) = DESPAWN_TAGS.lock() {
            tags.push(tag);
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
}

pub fn sync_camera_commands(cam_3d: &mut crate::render::Camera3D, cam_2d: &mut crate::render::Camera2D) {
    CAMERA_COMMANDS.with(|c| {
        let mut cmd = c.borrow_mut();
        if cmd.modified_3d {
            cam_3d.eye = glam::Vec3::new(cmd.pos.0, cmd.pos.1, cmd.pos.2);
            cam_3d.target = glam::Vec3::new(cmd.target.0, cmd.target.1, cmd.target.2);
            cam_3d.fovy = cmd.fov.to_radians();
            cmd.modified_3d = false;
        }
        if cmd.modified_2d {
            cam_2d.position = glam::Vec2::new(cmd.pos.0, cmd.pos.1);
            cam_2d.viewport_width = cmd.viewport.0;
            cam_2d.viewport_height = cmd.viewport.1;
            cmd.modified_2d = false;
        }
    });
}

pub fn update_scripts(host: &mut GrootScriptHost, world: &mut World, dt: f64) {
    host.reload_all();

    ENTITY_POSITIONS.with(|snapshot| {
        let mut map = snapshot.borrow_mut();
        map.clear();
        for (_entity, (comp, tf)) in world.query_mut::<(&GoScriptComponent, &Transform3D)>() {
            map.insert(comp.entity_id, (tf.position.x, tf.position.y, tf.position.z));
        }
    });

    TAG_POSITIONS.with(|snapshot| {
        let mut map = snapshot.borrow_mut();
        map.clear();
        for (_entity, (comp, tf)) in world.query_mut::<(&GoScriptComponent, &Transform3D)>() {
            map.entry(comp.tag.clone())
                .or_default()
                .push((tf.position.x, tf.position.y, tf.position.z));
        }
    });

    let mut despawn_list = Vec::new();

    for (entity, (comp, tf, vis3d, vis2d, vistext, collider)) in world.query_mut::<(
        &GoScriptComponent,
        &mut Transform3D,
        Option<&mut Visual3D>,
        Option<&mut Visual2D>,
        Option<&mut VisualText>,
        Option<&mut Collider>,
    )>() {
        CURRENT_ENTITY.with(|c| c.set(comp.entity_id));
        CURRENT_STATE.with(|st| {
            let mut s = st.borrow_mut();
            s.x = tf.position.x;
            s.y = tf.position.y;
            s.z = tf.position.z;
            s.pitch = tf.rotation.x;
            s.yaw = tf.rotation.y;
            s.roll = tf.rotation.z;
            s.scale_x = tf.scale.x;
            s.scale_y = tf.scale.y;
            s.scale_z = tf.scale.z;
            if let Some(v3) = &vis3d {
                s.color = (v3.color[0], v3.color[1], v3.color[2], v3.color[3]);
            } else if let Some(v2) = &vis2d {
                s.color = (v2.color[0], v2.color[1], v2.color[2], v2.color[3]);
            } else if let Some(vt) = &vistext {
                s.color = (vt.color[0], vt.color[1], vt.color[2], vt.color[3]);
            }
            if let Some(col) = &collider {
                s.collider = **col;
            }
            s.destroy_requested = false;
            s.text_value = None;
        });

        // Each entity runs the script on its own VM instance, so top-level
        // script state (`var self = ...`) is private to the entity.
        let vm = host.ensure_instance(&comp.script_path, comp.entity_id);

        if vm.globals.contains_key("OnUpdate") {
            vm.set_delta_time(dt);
            if let Err(e) = vm.call("OnUpdate", vec![Value::Float(dt)]) {
                log::error!("[GROOT SCRIPT ERROR] entity #{}: {e}", comp.entity_id);
            }
        }

        CURRENT_STATE.with(|st| {
            let s = st.borrow();
            if s.destroy_requested {
                despawn_list.push(entity);
            } else {
                tf.position = glam::Vec3::new(s.x, s.y, s.z);
                tf.rotation = glam::Vec3::new(s.pitch, s.yaw, s.roll);
                tf.scale = glam::Vec3::new(s.scale_x, s.scale_y, s.scale_z);
                if let Some(v3) = vis3d {
                    v3.color = [s.color.0, s.color.1, s.color.2, s.color.3];
                }
                if let Some(v2) = vis2d {
                    v2.color = [s.color.0, s.color.1, s.color.2, s.color.3];
                }
                if let Some(vt) = vistext {
                    vt.color = [s.color.0, s.color.1, s.color.2, s.color.3];
                    if let Some(txt) = &s.text_value {
                        vt.value = txt.clone();
                    }
                }
                if let Some(col) = collider {
                    *col = s.collider;
                }
            }
        });
    }

    for e in despawn_list {
        let _ = world.despawn(e);
    }

    if let Ok(mut reqs) = SET_TEXT_BY_TAG_REQUESTS.lock() {
        for (tag, text) in reqs.drain(..) {
            for (_entity, (comp, vis_text)) in world.query_mut::<(&GoScriptComponent, &mut VisualText)>() {
                if comp.tag == tag {
                    vis_text.value = text.clone();
                }
            }
            for (_entity, (tag_comp, vis_text)) in world.query_mut::<(&TagComponent, &mut VisualText)>() {
                if tag_comp.0 == tag {
                    vis_text.value = text.clone();
                }
            }
        }
    }

    if let Ok(mut tags) = DESPAWN_TAGS.lock() {
        for tag in tags.drain(..) {
            let ids: Vec<_> = world
                .query::<&GoScriptComponent>()
                .iter()
                .filter(|(_e, comp)| comp.tag == tag)
                .map(|(e, _)| e)
                .collect();
            for id in ids {
                let _ = world.despawn(id);
            }
        }
    }

    if let Ok(mut reqs) = SPAWN_REQUESTS.lock() {
        for req in reqs.drain(..) {
            match req {
                SpawnRequest::Prefab { path, x, y, z, tag } => {
                    if let Some(prefab) = PrefabConfig::load(&path) {
                        let scale = glam::Vec3::new(
                            prefab.transform.scale.0,
                            prefab.transform.scale.1,
                            prefab.transform.scale.2,
                        );
                        let rot = glam::Vec3::new(
                            prefab.transform.rotation.0,
                            prefab.transform.rotation.1,
                            prefab.transform.rotation.2,
                        );
                        let tf = Transform3D::new(glam::Vec3::new(x, y, z), rot, scale);
                        let visual_3d = prefab.visual.as_ref().and_then(|v| match v {
                            VisualConfig::MeshPbr { shape, material } => {
                                let shape = match shape {
                                    ShapeConfig::Cuboid { x, y, z } => {
                                        MeshShape::Cuboid { x: *x, y: *y, z: *z }
                                    }
                                    ShapeConfig::Sphere { radius } => MeshShape::Sphere { radius: *radius },
                                };
                                Some(Visual3D {
                                    shape,
                                    color: material.color.to_array(),
                                })
                            }
                            _ => None,
                        });
                        let visual_2d = prefab.visual.as_ref().and_then(|v| match v {
                            VisualConfig::Sprite { size, color, texture, layer } => Some(Visual2D {
                                size: *size,
                                color: color.to_array(),
                                texture_path: texture.clone(),
                                layer: *layer,
                            }),
                            _ => None,
                        });
                        let visual_text = prefab.visual.as_ref().and_then(|v| match v {
                            VisualConfig::Text { value, size, color, layer } => Some(VisualText {
                                value: value.clone(),
                                size: *size,
                                color: color.to_array(),
                                layer: *layer,
                            }),
                            _ => None,
                        });

                        let script = prefab.script.map(|s| GoScriptComponent {
                            script_path: s,
                            entity_id: NEXT_ENTITY_ID
                                .lock()
                                .map(|mut id| {
                                    let cur = *id;
                                    *id += 1;
                                    cur
                                })
                                .unwrap_or(20000),
                            tag,
                        });
                        let collider = prefab.collider.unwrap_or_default();

                        match (visual_3d, visual_2d, visual_text, script) {
                            (Some(v3), _, _, Some(sc)) => {
                                world.spawn((tf, v3, sc, collider));
                            }
                            (Some(v3), _, _, None) => {
                                world.spawn((tf, v3, collider));
                            }
                            (None, Some(v2), _, Some(sc)) => {
                                world.spawn((tf, v2, sc, collider));
                            }
                            (None, Some(v2), _, None) => {
                                world.spawn((tf, v2, collider));
                            }
                            (None, None, Some(vt), Some(sc)) => {
                                world.spawn((tf, vt, sc, collider));
                            }
                            (None, None, Some(vt), None) => {
                                world.spawn((tf, vt, collider));
                            }
                            (None, None, None, Some(sc)) => {
                                world.spawn((tf, sc, collider));
                            }
                            (None, None, None, None) => {
                                world.spawn((tf, collider));
                            }
                        }
                    }
                }
            }
        }
    }

    // Release the script instances of entities that no longer exist (they
    // were despawned, or their prefab was replaced).
    let live: HashSet<u32> = world
        .query::<&GoScriptComponent>()
        .iter()
        .map(|(_entity, comp)| comp.entity_id)
        .collect();
    host.prune_dead_entities(&live);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    /// Writes a temporary script file and returns its path.
    fn temp_script(name: &str, source: &str) -> PathBuf {
        let path = std::env::temp_dir().join(name);
        fs::write(&path, source).unwrap();
        path
    }

    /// Reads a global out of an entity's script instance.
    fn global_of(host: &GrootScriptHost, script_path: &str, entity_id: u32, name: &str) -> String {
        host.instance(script_path, entity_id)
            .unwrap_or_else(|| panic!("no instance for entity #{entity_id}"))
            .globals
            .get(name)
            .unwrap_or_else(|| panic!("no global '{name}'"))
            .to_string()
    }

    #[test]
    fn entities_get_isolated_script_state() {
        let path = temp_script(
            "groot_isolation_test.gos",
            "var counter = 0\n\
             func Bump() int {\n\
                 counter = counter + 1\n\
                 return counter\n\
             }\n",
        );
        let script_path = path.to_str().unwrap().to_string();

        let mut host = GrootScriptHost::new();

        // Entity #1 bumps twice, entity #2 bumps once.
        {
            let vm = host.ensure_instance(&script_path, 1);
            vm.call("Bump", vec![]).unwrap();
            vm.call("Bump", vec![]).unwrap();
        }
        {
            let vm = host.ensure_instance(&script_path, 2);
            vm.call("Bump", vec![]).unwrap();
        }

        // A shared VM would report 2 and 3 — each entity owns its counter.
        assert_eq!(global_of(&host, &script_path, 1, "counter"), "2");
        assert_eq!(global_of(&host, &script_path, 2, "counter"), "1");

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn hot_reload_swaps_code_but_keeps_entity_state() {
        let path = temp_script(
            "groot_hot_reload_test.gos",
            "var counter = 0\n\
             func Bump() int {\n\
                 counter = counter + 1\n\
                 return counter\n\
             }\n",
        );
        let script_path = path.to_str().unwrap().to_string();

        let mut host = GrootScriptHost::new();
        {
            let vm = host.ensure_instance(&script_path, 1);
            vm.call("Bump", vec![]).unwrap();
        }
        assert_eq!(global_of(&host, &script_path, 1, "counter"), "1");

        // Rewrite the script with a different body.
        std::thread::sleep(std::time::Duration::from_millis(20));
        fs::write(
            &path,
            "var counter = 0\n\
             func Bump() int {\n\
                 counter = counter + 10\n\
                 return counter\n\
             }\n",
        )
        .unwrap();

        host.reload_all();

        {
            let vm = host.ensure_instance(&script_path, 1);
            vm.call("Bump", vec![]).unwrap();
        }

        // The new body ran (1 + 10) and the live state survived the swap.
        assert_eq!(global_of(&host, &script_path, 1, "counter"), "11");

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn dead_entities_release_their_instances() {
        let path = temp_script(
            "groot_prune_test.gos",
            "func Bump() int { return 1 }\n",
        );
        let script_path = path.to_str().unwrap().to_string();

        let mut host = GrootScriptHost::new();
        host.ensure_instance(&script_path, 7);
        assert!(host.instance(&script_path, 7).is_some());

        let live: HashSet<u32> = HashSet::new();
        host.prune_dead_entities(&live);
        assert!(host.instance(&script_path, 7).is_none());

        let _ = fs::remove_file(&path);
    }
}
