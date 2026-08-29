# Groot

Groot is a fast-iteration 2D/3D game engine powered by **wgpu** (modern GPU rendering)
with **GoScript** — an embeddable Go-syntax scripting VM written in pure Rust.
Game logic lives in `.gos` files under `assets/scripts/` and hot-reloads on save;
visuals and prefab hierarchies live in RON asset files under `assets/prefabs/` and `assets/scenes/`.

## Architecture

Groot is **data-driven**: scripts own behavior and data; the host engine owns
representation and rendering. Visuals are declared as RON prefab data.

```
assets/scenes/*.scene.ron  ──►  hecs ECS (3D Meshes, Lights, Sprites, UI)
assets/prefabs/*.prefab.ron ──►  RonAssetWatcher (Visual Hot Reloading)
assets/scripts/*.gos        ──►  GoScript VM (Logic Hot Reloading)  ──► Entity Data
                                         ▲                                  │
                                         └── groot.* host bindings ──────────┘
```

- `assets/config.ron` — project config & render settings.
- `assets/prefabs/*.prefab.ron` — 2D/3D prefabs (sprites, text, 3D PBR meshes, lights, colliders, parent-child hierarchies).
- `assets/scenes/*.scene.ron` — scene layout, environment settings, cameras, entity initializers.
- `src/main.rs` — initializes winit window and wgpu render context, runs main event loop.
- `src/platform/` — platform abstraction layer (desktop event loop, Android entry).
- `src/render/` — pure wgpu rendering engine (3D meshes, 2D sprites, text, gizmos).
- `src/ecs/` — hecs-based ECS components and queries.
- `src/assets/` — asset loading, encryption, archive packing (see [Asset Security](#asset-security)).
- `src/script/` — GoScript VM host, input tracking, script execution.
- `src/plugin.rs` — plugin trait and manager (re-exports from `groot-plugin-api`).

## Asset Security

Groot ships a built-in asset protection pipeline that prevents plaintext
inspection of scripts, prefabs, and scenes in release builds.

### Encryption (`src/assets/crypto.rs`)

All assets are encrypted with a **ChaCha20-inspired XOR stream cipher** keyed
by a compile-time secret and an asset-path nonce. A CRC32 integrity header
detects tampering. The default key is baked in at compile time; production
builds can override it at compile time by setting the `GROOT_ENCRYPTION_KEY`
environment variable before building:

```bash
GROOT_ENCRYPTION_KEY="MyStrongKey32BytesExactly______!" cargo build --release
```

| Function | Description |
|---|---|
| `encrypt_asset(path, data)` | Encrypt bytes with a path-derived nonce |
| `decrypt_asset(path, data)` | Decrypt bytes; falls back to raw if not encrypted (debug) |
| `minify_goscript(source)` | Strip `//` and `/* */` comments + blank lines from `.gos` |

### Binary Archive — `.gpak` (`src/assets/pak.rs`)

Assets can be packed into a single encrypted **`.gpak`** binary archive,
similar to Unreal `.pak` or Unity asset bundles.

**Archive format:**

```
[GPAK]  4 bytes  magic
[u16]   2 bytes  version
[u32]   4 bytes  entry count
  per entry:
  [u16]   path length
  [N]     UTF-8 relative path
  [u32]   data length
  [M]     encrypted payload
```

Pack via the CLI (see [CLI](#cli)):

```bash
groot pack assets/ assets.gpak
```

Open a `.gpak` from Rust:

```rust
let archive = GpakArchive::open("assets.gpak")?;
let bytes = archive.entries.get("scripts/player.gos");
```

### In-Memory & Ephemeral Script Loading (`src/assets/embed.rs`)

| Build mode | Strategy |
|---|---|
| `cargo run` (debug, desktop) | Raw `.gos` read from disk — **hot-reload preserved** |
| `cargo build --release` | Decrypt + minify **in memory** via `HotReloadEngine::from_str` — no temp file |
| Android | Same as release — fully in-memory |

When a physical path is required (debug hot-reload watcher), `prepare_script_path`
writes a minified, hashed `.goc` file to `/tmp/.groot_cache/` with **`0600`
permissions** and registers a panic hook that deletes the cache on process exit.

## Plugin System

Groot supports native Rust plugins via the `GrootPlugin` trait. Plugins are
managed through the CLI and compiled as separate crates that depend on the
shared `groot-plugin-api` crate.

### Architecture

```
groot-plugin-api      ──►  Defines GrootPlugin trait + PluginManager
groot-plugin-audio    ──►  Sample audio synthesizer plugin
groot-plugin-gizmos   ──►  Sample debug shape drawer plugin
groot-plugins/        ──►  Plugin registry (index.ron)
```

### Writing a Plugin

```rust
use groot_plugin_api::{GrootPlugin, VirtualMachine};
use goscript::value::Value;

pub struct MyPlugin;

impl GrootPlugin for MyPlugin {
    fn name(&self) -> &'static str {
        "my-plugin"
    }

    fn register_script_bindings(&self, vm: &mut VirtualMachine) {
        vm.register_fn("my_plugin.DoThing", |args| {
            let x = args.first().and_then(|v| v.as_number()).unwrap_or(0.0);
            log::info!("[MY PLUGIN] Doing thing at {x}");
            Value::Nil
        });
    }
}
```

### CLI Plugin Commands

```bash
# List available plugins
groot plugin list

# Install a plugin (adds to Cargo.toml)
groot plugin add audio

# Remove a plugin
groot plugin remove audio
```

## CLI

```bash
# Install the CLI (binary is now `groot`)
cargo install --path .        # installs `groot` to ~/.cargo/bin

# Scaffold a new project folder
groot new my-game             # or: cargo run --bin groot -- new my-game

# Run the current Groot game
groot run                     # or: cargo run --bin groot -- run

# Build a release bundle
groot build                   # or: cargo run --bin groot -- build

# Pack and encrypt the assets directory into a .gpak archive
groot pack assets/ assets.gpak

# Scaffold prefabs, scripts, and scenes
groot generate prefab enemy
groot generate script player
groot generate scene arena    # alias: groot g <type> <name>

# Diagnose toolchain (Rust, cargo-apk, Android SDK/NDK, Vulkan)
groot doctor

# Shell completions
groot completions bash        # bash|zsh|fish
# or: groot completions zsh > ~/.zsh/completions/_groot

# Manage plugins
groot plugin list
groot plugin add audio
groot plugin remove audio
```

### Multi-Platform Targets

Desktop and Android builds share a single `--target` flag on `run` and `build` (defaults to `desktop`):

```bash
# Desktop (current host)
groot run
groot build --target desktop

# Android APK (requires rustup target aarch64-linux-android + cargo-apk)
groot run --target android    # cargo apk run (deploys to a connected device via adb)
groot build --target android  # cargo apk build --release
```

On Android, `groot run --target android` auto-detects connected devices with
`adb`; if several are present it lists them and lets you pick one, or you can
pass `--device <serial>` (or `--device <index>`) to select directly.

### Android Asset Handling

Assets (`assets/`) are compiled into the APK using `rust-embed` with the
`debug-embed` feature, so games work on-device in both debug and release
builds. On desktop debug builds, asset files are still read from disk first
so `.gos` and `.prefab.ron` edits hot-reload while developing.

### Android Requirements

- `rustup target add aarch64-linux-android`
- `cargo install cargo-apk`
- Android SDK with platform `android-34` (or set `target_sdk_version` /
  `min_sdk_version` under `[package.metadata.android.sdk]` in `Cargo.toml`)

## Run the Demo

```bash
cargo run
```

The **Flappy Groot** demo starts up: a neon 2D side-scroller where you flap a bird through pipe gaps, avoiding solid ground and ceiling. Save any `.gos` or `.prefab.ron` file to see live hot reloading!

2D sprites are drawn back-to-front by their `layer` field (background → pipes → ground/ceiling → bird) and scaled to their world-space size from the prefab's `size`. Sprite textures (bird, pipes, ground, ceiling, grid) are generated procedurally at startup and cached as PNG bind groups.

## Writing scripts

```go
type Player struct { Speed float64 }
var self = Player{Speed: 5.0}

func OnUpdate(dt float64) {
    var pos = groot.GetSelfPosition()
    var moveX = groot.GetAxis("Horizontal")
    var moveZ = -groot.GetAxis("Vertical")
    groot.SetSelfPosition(pos[0] + moveX*self.Speed*dt, pos[1], pos[2] + moveZ*self.Speed*dt)

    groot.SetSelfCollider(1.0, 1.0, 1.0)
    groot.Log("Hello from Groot 3D GoScript!")
}
```

## Dependencies

- `winit` 0.29 - Cross-platform windowing and input
- `wgpu` 0.19 - Modern GPU rendering (Vulkan/Metal/DX12)
- `glam` 0.27 - Fast 3D/2D math library
- `hecs` 0.10 - Minimalist archetype ECS
- `bytemuck` 1.14 - Safe casting for GPU buffers
- `pollster` 0.3 - Block on async operations
- `ron` 0.8 - Rusty Object Notation for assets
- `serde` 1 - Serialization framework
- `goscript` (git: `github.com/johnesleyer/goscript`) - GoScript VM
- `groot-plugin-api` - Shared plugin trait and manager

## Case Studies

Engineering deep-dives from the engine's development, including the Android
bring-up:

- [001 — Android splash-screen hang: deferred GPU surface creation](docs/case-studies/001-android-splash-screen-hang.md)
- [002 — Empty scene on Android: embedding assets into the binary](docs/case-studies/002-android-empty-scene-asset-embedding.md)
