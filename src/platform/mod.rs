pub mod android;
pub mod desktop;
pub mod web;

/// Unified source of truth for loading text assets on any platform.
///
/// On the Web (WASM) there is no synchronous filesystem, so assets always
/// resolve through the binary-embedded copy (`rust-embed`). On desktop and
/// Android, files are read from disk first (debug builds hot-reload from
/// `assets/`), then fall back to the embedded copy.
pub trait AssetLoader {
    fn load_text(&self, path: &str) -> Option<String>;
}

/// Platform-aware convenience wrapper around `crate::assets::embed`.
pub fn load_asset(path: &str) -> Option<String> {
    crate::assets::embed::load_asset_str(path)
}

/// Initialize the logging backend appropriate for the compiled platform.
pub fn init_platform_logging() {
    #[cfg(target_os = "android")]
    {
        android_logger::init_once(
            android_logger::Config::default().with_max_level(log::LevelFilter::Info),
        );
    }

    #[cfg(target_arch = "wasm32")]
    {
        std::panic::set_hook(Box::new(console_error_panic_hook::hook));
        console_log::init_with_level(log::Level::Info).expect("Failed to init console log");
    }

    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    {
        env_logger::builder()
            .filter_level(log::LevelFilter::Info)
            .init();
    }
}