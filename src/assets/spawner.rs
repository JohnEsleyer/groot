use crate::assets::ron_loader::*;
use crate::ecs::*;
use glam::Vec3;

pub fn spawn_scene(world: &mut World, scene_path: &str) {
    let scene = SceneConfig::load(scene_path);
    log::info!(
        "[GROOT SCENE] Spawning scene '{}' ({} entities)",
        scene.name,
        scene.entities.len()
    );

    for entity_cfg in &scene.entities {
        if let Some(prefab) = PrefabConfig::load(&entity_cfg.prefab) {
            let pos = Vec3::new(
                prefab.transform.position.0,
                prefab.transform.position.1,
                prefab.transform.position.2,
            );
            let rot = Vec3::new(
                prefab.transform.rotation.0,
                prefab.transform.rotation.1,
                prefab.transform.rotation.2,
            );
            let scale = Vec3::new(
                prefab.transform.scale.0,
                prefab.transform.scale.1,
                prefab.transform.scale.2,
            );

            let transform = Transform3D::new(pos, rot, scale);
            let collider = prefab.collider.unwrap_or_default();

            let visual_3d = prefab.visual.as_ref().and_then(|v| match v {
                VisualConfig::MeshPbr { shape, material } => {
                    let shape = match shape {
                        ShapeConfig::Cuboid { x, y, z } => MeshShape::Cuboid { x: *x, y: *y, z: *z },
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

            let entity_id = entity_cfg.entity_id.unwrap_or_else(rand_id);

            let script_comp = prefab.script.as_ref().map(|s| GoScriptComponent {
                script_path: s.clone(),
                entity_id,
                tag: entity_cfg.tag.clone(),
            });

            let tag_comp = if !entity_cfg.tag.is_empty() {
                Some(TagComponent(entity_cfg.tag.clone()))
            } else {
                None
            };

            match (visual_3d, visual_2d, visual_text, script_comp, tag_comp) {
                (Some(vis3d), _, _, Some(script), Some(tag)) => {
                    world.spawn((transform, vis3d, script, tag, collider));
                }
                (Some(vis3d), _, _, Some(script), None) => {
                    world.spawn((transform, vis3d, script, collider));
                }
                (Some(vis3d), _, _, None, Some(tag)) => {
                    world.spawn((transform, vis3d, tag, collider));
                }
                (Some(vis3d), _, _, None, None) => {
                    world.spawn((transform, vis3d, collider));
                }
                (None, Some(vis2d), _, Some(script), Some(tag)) => {
                    world.spawn((transform, vis2d, script, tag, collider));
                }
                (None, Some(vis2d), _, Some(script), None) => {
                    world.spawn((transform, vis2d, script, collider));
                }
                (None, Some(vis2d), _, None, Some(tag)) => {
                    world.spawn((transform, vis2d, tag, collider));
                }
                (None, Some(vis2d), _, None, None) => {
                    world.spawn((transform, vis2d, collider));
                }
                (None, None, Some(vistext), Some(script), Some(tag)) => {
                    world.spawn((transform, vistext, script, tag, collider));
                }
                (None, None, Some(vistext), Some(script), None) => {
                    world.spawn((transform, vistext, script, collider));
                }
                (None, None, Some(vistext), None, Some(tag)) => {
                    world.spawn((transform, vistext, tag, collider));
                }
                (None, None, Some(vistext), None, None) => {
                    world.spawn((transform, vistext, collider));
                }
                (None, None, None, Some(script), Some(tag)) => {
                    world.spawn((transform, script, tag, collider));
                }
                (None, None, None, Some(script), None) => {
                    world.spawn((transform, script, collider));
                }
                (None, None, None, None, Some(tag)) => {
                    world.spawn((transform, tag, collider));
                }
                (None, None, None, None, None) => {
                    world.spawn((transform, collider));
                }
            }
        } else {
            log::warn!("[GROOT SCENE] Could not load prefab asset '{}'", entity_cfg.prefab);
        }
    }
}

fn rand_id() -> u32 {
    use std::sync::atomic::{AtomicU32, Ordering};
    static NEXT_ID: AtomicU32 = AtomicU32::new(10_000);
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}