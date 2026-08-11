use rust_embed::RustEmbed;
#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;

#[derive(RustEmbed)]
#[folder = "assets/"]
pub struct EmbeddedAssets;

fn normalize_asset_path(path: &str) -> String {
    let clean = path.replace('\\', "/");
    clean
        .strip_prefix("assets/")
        .unwrap_or(&clean)
        .to_string()
}

pub fn load_asset_str(path: &str) -> Option<String> {
    #[cfg(not(target_arch = "wasm32"))]
    #[cfg(debug_assertions)]
    {
        if Path::new(path).exists() {
            if let Ok(content) = std::fs::read_to_string(path) {
                return Some(content);
            }
        }
    }

    let relative = normalize_asset_path(path);
    if let Some(file) = EmbeddedAssets::get(&relative) {
        if let Ok(content) = std::str::from_utf8(&file.data) {
            return Some(content.to_string());
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        std::fs::read_to_string(path).ok()
    }
    #[cfg(target_arch = "wasm32")]
    {
        None
    }
}

pub fn prepare_script_path(path: &str) -> String {
    #[cfg(not(target_arch = "wasm32"))]
    #[cfg(debug_assertions)]
    {
        if Path::new(path).exists() {
            return path.to_string();
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        if Path::new(path).exists() {
            return path.to_string();
        }

        let relative = normalize_asset_path(path);
        if let Some(file) = EmbeddedAssets::get(&relative) {
            let temp_dir = std::env::temp_dir().join("groot_runtime_scripts");
            let target_path = temp_dir.join(&relative);
            if let Some(parent) = target_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if std::fs::write(&target_path, &file.data).is_ok() {
                return target_path.to_string_lossy().to_string();
            }
        }
    }

    path.to_string()
}
