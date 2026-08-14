use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct GrootConfig {
    pub project: ProjectConfig,
    pub window: WindowConfig,
    #[serde(default)]
    pub render: RenderConfig,
    pub initial_scene: String,
}

impl GrootConfig {
    pub fn load(path: &str) -> Self {
        let raw = match crate::assets::load_asset_str(path) {
            Some(raw) => raw,
            None => {
                log::warn!("[GROOT CONFIG] Missing '{path}'; using fallback");
                return Self::default();
            }
        };
        match ron::from_str(&raw) {
            Ok(cfg) => cfg,
            Err(e) => {
                log::error!("[GROOT CONFIG] Parse error in '{path}': {e}; using fallback");
                Self::default()
            }
        }
    }
}

impl Default for GrootConfig {
    fn default() -> Self {
        Self {
            project: ProjectConfig {
                name: "Groot Engine".into(),
                version: "0.2.0".into(),
            },
            window: WindowConfig {
                title: "Groot Engine".into(),
                width: 1280.0,
                height: 720.0,
            },
            render: RenderConfig::default(),
            initial_scene: "assets/scenes/main.scene.ron".into(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ProjectConfig {
    pub name: String,
    pub version: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WindowConfig {
    pub title: String,
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RenderConfig {
    #[serde(default)]
    pub clear_color: ClearColorConfig,
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            clear_color: ClearColorConfig::Tuple((0.08, 0.09, 0.12, 1.0)),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum ClearColorConfig {
    Tuple((f64, f64, f64, f64)),
    Rgba(RgbaColor),
}

impl Default for ClearColorConfig {
    fn default() -> Self {
        ClearColorConfig::Tuple((0.08, 0.09, 0.12, 1.0))
    }
}

impl ClearColorConfig {
    pub fn to_tuple(&self) -> (f64, f64, f64, f64) {
        match self {
            ClearColorConfig::Tuple(t) => *t,
            ClearColorConfig::Rgba(RgbaColor::Rgba(r, g, b, a)) => {
                (*r as f64, *g as f64, *b as f64, *a as f64)
            }
            ClearColorConfig::Rgba(RgbaColor::Rgb(r, g, b)) => {
                (*r as f64, *g as f64, *b as f64, 1.0)
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum RgbaColor {
    Rgba(f32, f32, f32, f32),
    Rgb(f32, f32, f32),
}

impl RgbaColor {
    pub fn to_array(&self) -> [f32; 4] {
        match *self {
            RgbaColor::Rgba(r, g, b, a) => [r, g, b, a],
            RgbaColor::Rgb(r, g, b) => [r, g, b, 1.0],
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, Default)]
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

fn default_sprite_size() -> (f32, f32) {
    (1.0, 1.0)
}

fn default_text_size() -> f32 {
    18.0
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum VisualConfig {
    Sprite {
        #[serde(default = "default_sprite_size")]
        size: (f32, f32),
        color: RgbaColor,
        #[serde(default)]
        texture: Option<String>,
        #[serde(default)]
        layer: i32,
    },
    Text {
        value: String,
        #[serde(default = "default_text_size")]
        size: f32,
        color: RgbaColor,
        #[serde(default)]
        layer: i32,
    },
    MeshPbr {
        shape: ShapeConfig,
        material: MaterialConfig,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum ShapeConfig {
    Cuboid { x: f32, y: f32, z: f32 },
    Sphere { radius: f32 },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MaterialConfig {
    pub color: RgbaColor,
    #[serde(default)]
    pub roughness: f32,
    #[serde(default)]
    pub metallic: f32,
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
    pub collider: Option<crate::ecs::Collider>,
}

impl PrefabConfig {
    pub fn load(path: &str) -> Option<Self> {
        let raw = crate::assets::load_asset_str(path)?;
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
        let raw = match crate::assets::load_asset_str(path) {
            Some(raw) => raw,
            None => {
                log::warn!("[GROOT SCENE] Cannot read '{path}'");
                return Self::default();
            }
        };
        match ron::from_str(&raw) {
            Ok(cfg) => cfg,
            Err(e) => {
                log::error!("[GROOT SCENE] Error parsing '{path}': {e}");
                Self::default()
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, Default)]
pub struct EnvironmentConfig {
    #[serde(default)]
    pub ambient_light: Option<AmbientLightConfig>,
    #[serde(default)]
    pub camera: Option<CameraConfig>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AmbientLightConfig {
    pub color: RgbaColor,
    #[serde(default)]
    pub brightness: f32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum CameraConfig {
    Perspective3D {
        fov: f32,
        position: (f32, f32, f32),
        look_at: (f32, f32, f32),
    },
    Orthographic2D {
        viewport: (f32, f32),
        position: (f32, f32),
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SceneEntityConfig {
    pub prefab: String,
    #[serde(default)]
    pub entity_id: Option<u32>,
    #[serde(default)]
    pub tag: String,
}

#[cfg(test)]
mod parse_tests {
    use super::*;

    #[test]
    fn config_parses_flappy_scene() {
        let cfg = GrootConfig::load("assets/config.ron");
        assert_eq!(cfg.project.name, "Flappy Groot Demo");
        assert_eq!(cfg.initial_scene, "assets/scenes/flappy.scene.ron");
        let (r, g, b, _a) = cfg.render.clear_color.to_tuple();
        assert!(r < 0.05 && g < 0.05 && b < 0.05, "dark neon clear color");
    }

    #[test]
    fn scene_parses_with_environment() {
        let scene = SceneConfig::load("assets/scenes/flappy.scene.ron");
        assert!(
            scene.environment.ambient_light.is_some(),
            "ambient_light captured"
        );
        assert!(scene.environment.camera.is_some(), "camera captured");
        assert_eq!(scene.entities.len(), 6);
    }

    #[test]
    fn scene_parses_orthographic_2d() {
        let scene = SceneConfig::load("assets/scenes/flappy_2d.scene.ron");
        assert!(
            matches!(
                scene.environment.camera,
                Some(CameraConfig::Orthographic2D { viewport, .. }) if viewport == (21.33, 12.0)
            ),
            "orthographic 2D camera with world viewport"
        );
        assert_eq!(scene.entities.len(), 6);
    }

    #[test]
    fn sprite_prefab_parses() {
        let prefab = PrefabConfig::load("assets/prefabs/bird_2d.prefab.ron")
            .expect("bird_2d prefab loads");
        assert!(
            matches!(
                prefab.visual,
                Some(VisualConfig::Sprite { size, texture, .. })
                    if size == (1.2, 1.2) && texture.as_deref() == Some("assets/textures/bird.png")
            ),
            "sprite visual with size + texture captured"
        );
    }
}
