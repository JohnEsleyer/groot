# Case Study 002: Empty Scene on Android — Embedding Assets Into the Binary

## TL;DR

The Android APK launched, the GPU initialized, and the renderer presented
frames — but the screen showed nothing. The log revealed the root cause:

```
W groot::assets::ron_lo..: [GROOT CONFIG] Missing 'assets/config.ron'; using fallback
W groot::assets::ron_lo..: [GROOT SCENE] Cannot read 'assets/scenes/main.scene.ron'
I groot::assets::spawner: [GROOT SCENE] Spawning scene '' (0 entities)
```

`rust-embed` in debug builds reads assets from the host filesystem at the
configured folder path. On Android that path points at the Linux dev machine,
which doesn't exist on the phone, so every asset lookup returned `None`. The
scene had zero entities, so nothing rendered.

The fix forces assets to be compiled directly into `libgroot.so` in all build
profiles.

## Symptoms

- APK installs and launches; no crash, no panic, no log error from the engine.
- The wgpu pipeline initializes (Vulkan adapter `Mali-G57 MC2` is found).
- The screen stays blank (or shows only the clear color) because the spawned
  scene has 0 entities.

## Root Cause

`rust-embed` generates two code paths depending on the build profile:

- **Release** (`cfg(not(debug_assertions))`): files under `assets/` are
  embedded as `static &[u8]` bytes via `include_bytes!` inside the binary.
- **Debug** (default): files are *not* embedded; instead `get()` reads from
  the host filesystem at the `#[folder = "assets/"]` path at runtime.

So in a debug APK the embedded code path is compiled out entirely. The
`get()` lookup reads `/media/john/.../groot/assets/` — a Linux path that does
not exist on the Android device — and returns `None`. Every `load_asset_str`
call fell through to `std::fs::read_to_string`, which also failed, so the
config fell back to defaults and the scene spawner loaded an empty scene.

## The Dead-End: `#[rust_embed(debug = false)]`

The initial plan was to annotate the embedded struct:

```rust
#[derive(RustEmbed)]
#[folder = "assets/"]
#[rust_embed(debug = false)]   // <-- attribute does NOT exist
pub struct EmbeddedAssets;
```

This fails to compile with:

```
error: cannot find attribute `rust_embed` in this scope
```

`rust-embed` 8.x does not support a `debug` attribute. The derive macro only
registers `folder`, `prefix`, `include`, `exclude`, `allow_missing`,
`metadata_only`, `crate_path`, and `compression`. Whether assets are embedded
in debug builds is controlled by the crate's **`debug-embed` feature flag**
(`if cfg!(feature = "debug-embed")` in the macro), not an attribute.

## Fix: Enable the `debug-embed` Feature

Enable the feature on the dependency in `Cargo.toml`:

```toml
rust-embed = { version = "8.4", features = ["debug-embed"] }
```

With `debug-embed` on, the macro emits the static embedded byte arrays
unconditionally — in both debug and release builds — so `get()` always serves
assets straight from the binary.

### Asset Loading Fallback Behavior

`src/assets/embed.rs` keeps a filesystem fast-path for developer ergonomics,
now restricted to non-Android debug builds:

```rust
#[cfg(all(debug_assertions, not(target_os = "android")))]
{
    if Path::new(path).exists() {
        if let Ok(content) = std::fs::read_to_string(path) {
            return Some(content);
        }
    }
}
```

On desktop debug builds, files are still read from disk first so editing
`.gos`/`.prefab.ron` hot-reloads. On Android, that block is compiled out and
the embedded bytes are always used.

## Result

- `assets/config.ron`, the initial scene, and GoScript files load from
  embedded bytes on Android.
- The demo scene now spawns its entities and renders on the device.
- Desktop hot-reload behavior is unchanged.

## Notes & Caveats

- Embedding assets into `libgroot.so` increases the binary/APK size.
  Consider splitting large, content-heavy assets out later (e.g. stream from
  the device filesystem after first install).
- `prepare_script_path` writes embedded `.gos` files to the OS temp directory
  so the GoScript VM can open them by path; on Android that is a valid
  writable location (`/data/user/0/rust.groot/...` via `std::env::temp_dir()`).

## Files Touched

- `Cargo.toml` / `Cargo.lock` — added `debug-embed` feature to `rust-embed`.
- `src/assets/embed.rs` — restricted the debug filesystem fast-path to
  non-Android targets.
