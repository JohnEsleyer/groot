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
        ron::from_str(&raw).unwrap_or_default()
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
            render: RenderConfig {
                clear_color: (0.08, 0.09, 0.12, 1.0),
            },
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
    pub clear_color: (f64, f64, f64, f64),
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            clear_color: (0.08, 0.09, 0.12, 1.0),
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

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum VisualConfig {
    Sprite {
        size: (f32, f32),
        color: RgbaColor,
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
        ron::from_str(&raw).unwrap_or_default()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SceneEntityConfig {
    pub prefab: String,
    #[serde(default)]
    pub entity_id: Option<u32>,
    #[serde(default)]
    pub tag: String,
}