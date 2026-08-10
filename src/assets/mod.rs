pub mod embed;
pub mod ron_loader;
pub mod spawner;

pub use embed::{load_asset_str, prepare_script_path};
pub use ron_loader::*;
pub use spawner::spawn_scene;