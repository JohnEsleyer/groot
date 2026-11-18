use glam::{Mat4, Vec2, Vec3};

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CameraUniform {
    pub view_proj: [[f32; 4]; 4],
    pub camera_pos: [f32; 4],
}

impl Default for CameraUniform {
    fn default() -> Self {
        Self {
            view_proj: Mat4::IDENTITY.to_cols_array_2d(),
            camera_pos: [0.0, 0.0, 0.0, 1.0],
        }
    }
}

pub struct Camera3D {
    pub eye: Vec3,
    pub target: Vec3,
    pub up: Vec3,
    pub aspect: f32,
    pub fovy: f32,
    pub znear: f32,
    pub zfar: f32,
}

impl Camera3D {
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            eye: Vec3::new(0.0, 0.0, 10.0),
            target: Vec3::ZERO,
            up: Vec3::Y,
            aspect: width / height.max(1.0),
            fovy: 60.0f32.to_radians(),
            znear: 0.1,
            zfar: 100.0,
        }
    }

    pub fn build_view_projection_matrix(&self) -> Mat4 {
        let view = Mat4::look_at_rh(self.eye, self.target, self.up);
        let proj = Mat4::perspective_rh(self.fovy, self.aspect, self.znear, self.zfar);
        proj * view
    }

    pub fn build_uniform(&self) -> CameraUniform {
        CameraUniform {
            view_proj: self.build_view_projection_matrix().to_cols_array_2d(),
            camera_pos: [self.eye.x, self.eye.y, self.eye.z, 1.0],
        }
    }
}

pub struct Camera2D {
    pub position: Vec2,
    pub viewport_width: f32,
    pub viewport_height: f32,
    pub znear: f32,
    pub zfar: f32,
}

impl Camera2D {
    /// Creates an orthographic camera that shows a fixed world-space viewport
    /// height of 12.0 units. World-space widths are derived from the window
    /// aspect ratio so a 1.2-unit bird occupies ~10% of the screen height.
    pub fn new(window_width: f32, window_height: f32) -> Self {
        let world_height = 12.0f32; // 12 units high (fits 7.0-unit pipes cleanly)
        let aspect = window_width / window_height.max(1.0);
        let world_width = world_height * aspect;

        Self {
            position: Vec2::ZERO,
            viewport_width: world_width.max(1.0),
            viewport_height: world_height,
            znear: -100.0,
            zfar: 100.0,
        }
    }

    pub fn build_view_projection_matrix(&self) -> Mat4 {
        let half_w = self.viewport_width * 0.5;
        let half_h = self.viewport_height * 0.5;
        Mat4::orthographic_rh(
            self.position.x - half_w,
            self.position.x + half_w,
            self.position.y - half_h,
            self.position.y + half_h,
            self.znear,
            self.zfar,
        )
    }

    pub fn build_uniform(&self) -> CameraUniform {
        CameraUniform {
            view_proj: self.build_view_projection_matrix().to_cols_array_2d(),
            camera_pos: [self.position.x, self.position.y, 0.0, 1.0],
        }
    }
}