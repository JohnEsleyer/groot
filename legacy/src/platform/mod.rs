pub mod android;
pub mod desktop;

/// Unified source of truth for loading text assets on any platform.
///
/// Files are read from disk first (debug builds hot-reload from `assets/`),
/// then fall back to the embedded copy.
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

    #[cfg(not(target_os = "android"))]
    {
        env_logger::builder()
            .filter_level(log::LevelFilter::Info)
            .init();
    }
}
