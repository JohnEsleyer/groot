use glam::{Mat4, Quat, Vec3};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct Transform3D {
    pub position: Vec3,
    pub rotation: Vec3,
    pub scale: Vec3,
}

impl Transform3D {
    pub fn new(position: Vec3, rotation: Vec3, scale: Vec3) -> Self {
        Self {
            position,
            rotation,
            scale,
        }
    }

    pub fn matrix(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(
            self.scale,
            Quat::from_euler(
                glam::EulerRot::YXZ,
                self.rotation.y.to_radians(),
                self.rotation.x.to_radians(),
                self.rotation.z.to_radians(),
            ),
            self.position,
        )
    }
}

impl Default for Transform3D {
    fn default() -> Self {
        Self {
            position: Vec3::ZERO,
            rotation: Vec3::ZERO,
            scale: Vec3::ONE,
        }
    }
}

#[derive(Debug, Clone)]
pub enum MeshShape {
    Cuboid { x: f32, y: f32, z: f32 },
    Sphere { radius: f32 },
}

#[derive(Debug, Clone)]
pub struct Visual3D {
    pub shape: MeshShape,
    pub color: [f32; 4],
}

#[derive(Debug, Clone)]
pub struct GoScriptComponent {
    pub script_path: String,
    pub entity_id: u32,
    pub tag: String,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
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