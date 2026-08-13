use rust_embed::RustEmbed;
use std::path::Path;

#[derive(RustEmbed)]
#[folder = "assets/"]
pub struct EmbeddedAssets;

fn normalize_asset_path(path: &str) -> String {
    let clean = path.replace('\\', "/");
    let trimmed = clean.trim_start_matches('/');
    if let Some(stripped) = trimmed.strip_prefix("assets/") {
        stripped.to_string()
    } else {
        trimmed.to_string()
    }
}

pub fn load_asset_str(path: &str) -> Option<String> {
    #[cfg(all(debug_assertions, not(target_os = "android")))]
    {
        if Path::new(path).exists() {
            if let Ok(content) = std::fs::read_to_string(path) {
                return Some(content);
            }
        }
    }

    // Dynamic rust-embed lookup fallback
    let clean = path.replace('\\', "/");
    let trimmed = clean.trim_start_matches('/');
    let relative = normalize_asset_path(path);

    let candidates = [
        relative.as_str(),
        trimmed,
        path,
    ];

    for candidate in candidates {
        if let Some(file) = EmbeddedAssets::get(candidate) {
            if let Ok(content) = std::str::from_utf8(&file.data) {
                return Some(content.to_string());
            }
        }
    }

    std::fs::read_to_string(path).ok()
}

pub fn load_asset_bytes(path: &str) -> Option<Vec<u8>> {
    #[cfg(all(debug_assertions, not(target_os = "android")))]
    {
        if Path::new(path).exists() {
            if let Ok(content) = std::fs::read(path) {
                return Some(content);
            }
        }
    }

    let clean = path.replace('\\', "/");
    let trimmed = clean.trim_start_matches('/');
    let relative = normalize_asset_path(path);

    let candidates = [relative.as_str(), trimmed, path];

    for candidate in candidates {
        if let Some(file) = EmbeddedAssets::get(candidate) {
            return Some(file.data.to_vec());
        }
    }

    std::fs::read(path).ok()
}

pub fn prepare_script_path(path: &str) -> String {
    #[cfg(all(debug_assertions, not(target_os = "android")))]
    {
        if Path::new(path).exists() {
            return path.to_string();
        }
    }

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

    path.to_string()
}
