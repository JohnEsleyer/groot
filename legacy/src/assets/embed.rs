//! Embedded asset loader with transparent decrypt-on-demand support.
//!
//! In debug builds on desktop, raw files are read from disk directly for
//! hot-reloading.  In release (or Android) builds the assets are embedded via
//! `rust_embed` and decrypted in memory using `crypto::decrypt_asset`.
//!
//! `prepare_script_path` returns a **physical path** that GoScript's
//! `HotReloadEngine` can watch.  In debug mode it is the unmodified source
//! path; in release mode the decrypted, minified source is written to a
//! randomised ephemeral file under `~/.groot_cache/` with `0600` permissions
//! and cleaned up on engine exit.

use rust_embed::RustEmbed;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use super::crypto::{decrypt_asset, minify_goscript};

#[derive(RustEmbed)]
#[folder = "assets/"]
pub struct EmbeddedAssets;

static CLEANUP_REGISTERED: AtomicBool = AtomicBool::new(false);

fn normalize_asset_path(path: &str) -> String {
    let clean = path.replace('\\', "/");
    let trimmed = clean.trim_start_matches('/');
    if let Some(stripped) = trimmed.strip_prefix("assets/") {
        stripped.to_string()
    } else {
        trimmed.to_string()
    }
}

/// Load an embedded asset as a UTF-8 string.
///
/// In debug desktop builds the file is read directly from disk.
/// Otherwise it is decrypted from the embedded bytes.
pub fn load_asset_str(path: &str) -> Option<String> {
    #[cfg(all(debug_assertions, not(target_os = "android")))]
    {
        if Path::new(path).exists() {
            if let Ok(content) = std::fs::read_to_string(path) {
                return Some(content);
            }
        }
    }

    let bytes = load_asset_bytes(path)?;
    String::from_utf8(bytes).ok()
}

/// Load an embedded asset as raw bytes, decrypting on demand.
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
            // Decrypt the payload.  For unencrypted debug builds
            // `decrypt_asset` transparently returns the raw bytes.
            return decrypt_asset(candidate, &file.data);
        }
    }

    // Last resort: read from disk and attempt to decrypt.
    if let Ok(data) = std::fs::read(path) {
        return decrypt_asset(path, &data);
    }

    None
}

/// Prepare a physical script path for `HotReloadEngine`.
///
/// * **Debug / desktop** – returns the original path so the engine can watch it.
/// * **Release / Android** – decrypts + minifies the source, writes it to an
///   ephemeral randomised file under `~/.groot_cache/` with `0600` Unix
///   permissions, registers a panic-hook that deletes the cache directory on
///   process exit, and returns the temp path.
pub fn prepare_script_path(path: &str) -> String {
    #[cfg(all(debug_assertions, not(target_os = "android")))]
    {
        if Path::new(path).exists() {
            return path.to_string();
        }
    }

    let relative = normalize_asset_path(path);

    if let Some(raw_bytes) = load_asset_bytes(path) {
        let script_source = String::from_utf8_lossy(&raw_bytes).to_string();
        let sanitized = minify_goscript(&script_source);

        // Randomised temp location to prevent path prediction.
        let hash_name = format!("{:x}.goc", calculate_simple_hash(relative.as_bytes()));
        let temp_dir = std::env::temp_dir().join(".groot_cache");
        let _ = std::fs::create_dir_all(&temp_dir);
        let target_path = temp_dir.join(&hash_name);

        if let Ok(mut file) = std::fs::File::create(&target_path) {
            use std::io::Write;
            let _ = file.write_all(sanitized.as_bytes());

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(
                    &target_path,
                    std::fs::Permissions::from_mode(0o600),
                );
            }
        }

        register_cleanup(&temp_dir);
        return target_path.to_string_lossy().to_string();
    }

    path.to_string()
}

/// Load the source of a GoScript asset entirely in memory (no temp file).
///
/// Returns the decrypted, minified source string, or `None` if the asset
/// cannot be found.
pub fn load_script_source(path: &str) -> Option<String> {
    #[cfg(all(debug_assertions, not(target_os = "android")))]
    {
        if Path::new(path).exists() {
            if let Ok(src) = std::fs::read_to_string(path) {
                return Some(src);
            }
        }
    }

    let raw_bytes = load_asset_bytes(path)?;
    let source = String::from_utf8(raw_bytes).ok()?;
    Some(minify_goscript(&source))
}

fn calculate_simple_hash(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

fn register_cleanup(cache_dir: &Path) {
    if !CLEANUP_REGISTERED.swap(true, Ordering::SeqCst) {
        let dir = cache_dir.to_path_buf();
        std::panic::set_hook(Box::new(move |_| {
            let _ = std::fs::remove_dir_all(&dir);
        }));
    }
}
