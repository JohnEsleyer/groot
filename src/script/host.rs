use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Mutex;

use goscript::value::Value;
use goscript::HotReloadEngine;
use hecs::World;

use crate::assets::ron_loader::{PrefabConfig, ShapeConfig, VisualConfig};
use crate::ecs::*;
use crate::groot_module::GrootModuleExt;
use crate::plugin::PluginManager;
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

                let inp_mouse_down = Rc::clone(&input_ref);
                vm.register_fn("groot.IsMouseDown", move |args| {
                    let btn = args.first().and_then(|v| v.as_number()).unwrap_or(0.0) as usize;
                    if btn < 3 {
                        return Value::Bool(inp_mouse_down.mouse_button_down.get()[btn]);
                    }
                    Value::Bool(false)
                });

                let inp_mouse_pressed = Rc::clone(&input_ref);
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
    host.plugin_mgr.update_all(world, dt);

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
}
