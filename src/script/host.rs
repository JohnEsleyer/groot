use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Mutex;

use goscript::value::Value;
use goscript::HotReloadEngine;
use hecs::World;

use crate::assets::ron_loader::PrefabConfig;
use crate::ecs::*;
use crate::groot_module::GrootModuleExt;
use crate::plugin::PluginManager;
use crate::script::input::InputState;

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

#[derive(Clone, Debug)]
pub enum SpawnRequest {
    Prefab { path: String, x: f32, y: f32, z: f32, tag: String },
}

static SPAWN_REQUESTS: Mutex<Vec<SpawnRequest>> = Mutex::new(Vec::new());

/// Cached metadata for a loaded script — tracks which lifecycle hooks are defined.
#[derive(Debug, Clone, Default)]
pub struct ScriptMetadata {
    pub has_on_update: bool,
    pub has_on_start: bool,
}

pub struct LoadedScript {
    pub engine: HotReloadEngine,
    pub metadata: ScriptMetadata,
}

pub struct GrootScriptHost {
    scripts: HashMap<String, LoadedScript>,
    pub input: Rc<InputState>,
    pub plugin_mgr: PluginManager,
}

impl GrootScriptHost {
    pub fn new() -> Self {
        Self {
            scripts: HashMap::new(),
            input: Rc::new(InputState::new()),
            plugin_mgr: PluginManager::new(),
        }
    }

    pub fn ensure_engine(&mut self, script_path: &str) -> &mut LoadedScript {
        let _resolved_path = crate::assets::prepare_script_path(script_path);
        let input_ref = Rc::clone(&self.input);
        let plugin_mgr_ref = &self.plugin_mgr;

        self.scripts
            .entry(script_path.to_string())
            .or_insert_with(|| {
                let mut engine = HotReloadEngine::new(&_resolved_path);

                let vm = &mut engine.vm;

                vm.register_groot_module();
                plugin_mgr_ref.register_all_script_bindings(vm);

                let inp_axis = Rc::clone(&input_ref);
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

                let inp_key = Rc::clone(&input_ref);
                vm.register_fn("groot.IsKeyDown", move |args| {
                    if let Some(key) = args.first().and_then(|v| v.as_string()) {
                        return Value::Bool(inp_key.keys_down.borrow().iter().any(|k| k == key));
                    }
                    Value::Bool(false)
                });

                let inp_pressed = Rc::clone(&input_ref);
                vm.register_fn("groot.IsKeyPressed", move |args| {
                    if let Some(key) = args.first().and_then(|v| v.as_string()) {
                        return Value::Bool(inp_pressed.keys_just_pressed.borrow().iter().any(|k| k == key));
                    }
                    Value::Bool(false)
                });

                vm.register_fn("groot.GetSelfEntity", |_| {
                    Value::Int(CURRENT_ENTITY.with(|c| c.get()) as i64)
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

                let _ = engine.reload_if_changed();

                let metadata = ScriptMetadata {
                    has_on_update: engine.vm.globals.contains_key("OnUpdate"),
                    has_on_start: engine.vm.globals.contains_key("OnStart"),
                };

                LoadedScript { engine, metadata }
            })
    }

    pub fn reload_all(&mut self) {
        for loaded in self.scripts.values_mut() {
            if let Ok(reloaded) = loaded.engine.reload_if_changed() {
                if reloaded {
                    loaded.metadata.has_on_update = loaded.engine.vm.globals.contains_key("OnUpdate");
                    loaded.metadata.has_on_start = loaded.engine.vm.globals.contains_key("OnStart");
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

pub fn update_scripts(host: &mut GrootScriptHost, world: &mut World, dt: f64) {
    host.reload_all();
    host.plugin_mgr.update_all(world, dt);

    ENTITY_POSITIONS.with(|snapshot| {
        let mut map = snapshot.borrow_mut();
        map.clear();
        for (_entity, (comp, tf)) in world.query_mut::<(&GoScriptComponent, &Transform3D)>() {
            map.insert(comp.entity_id, (tf.position.x, tf.position.y, tf.position.z));
        }
    });

    let mut despawn_list = Vec::new();

    for (entity, (comp, tf, visual, collider)) in world.query_mut::<(
        &GoScriptComponent,
        &mut Transform3D,
        Option<&mut Visual3D>,
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
            if let Some(vis) = &visual {
                s.color = (vis.color[0], vis.color[1], vis.color[2], vis.color[3]);
            }
            if let Some(col) = &collider {
                s.collider = **col;
            }
            s.destroy_requested = false;
        });

        let loaded = host.ensure_engine(&comp.script_path);

        if loaded.metadata.has_on_update {
            loaded.engine.vm.set_delta_time(dt);
            if let Err(e) = loaded.engine.vm.call("OnUpdate", vec![Value::Float(dt)]) {
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
                if let Some(vis) = visual {
                    vis.color = [s.color.0, s.color.1, s.color.2, s.color.3];
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

    if let Ok(mut reqs) = SPAWN_REQUESTS.lock() {
        for req in reqs.drain(..) {
            match req {
                SpawnRequest::Prefab { path, x, y, z, tag } => {
                    if let Some(prefab) = PrefabConfig::load(&path) {
                        let tf = Transform3D::new(glam::Vec3::new(x, y, z), glam::Vec3::ZERO, glam::Vec3::ONE);
                        let script = prefab.script.map(|s| GoScriptComponent {
                            script_path: s,
                            entity_id: 20000,
                            tag,
                        });
                        if let Some(sc) = script {
                            world.spawn((tf, sc, Collider::default()));
                        }
                    }
                }
            }
        }
    }
}
