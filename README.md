# Groot Engine (Ube Edition)

**Groot** is a zero-GC 2D/3D game engine written directly in **Ube**, targeting the **WASI 0.2 Component Model** and backed by a high-throughput native Rust host (`winit` + `wgpu` + `wasmtime`).

The previous GoScript-based engine is preserved under `legacy/` (and on the `legacy-goscript` git branch) for reference.

---

## Architecture Overview

```
┌───────────────────────────────────────────────────────────────┐
│                     GAME APPLICATION (Ube)                    │
│        gameplay, scenes, animation, UI, spawn logic           │
└──────────────────────────────┬────────────────────────────────┘
                               │
┌──────────────────────────────▼────────────────────────────────┐
│                     GROOT CORE (Ube)                          │
│  math · color · render · input · ecs · physics2d · assets     │
│  time · app · platform                                        │
└──────────────────────────────┬────────────────────────────────┘
                               │ WASI 0.2 component imports
┌──────────────────────────────▼────────────────────────────────┐
│                  NATIVE HOST (Rust)                           │
│  winit window/events · wgpu sprite batches · wasmtime + WASI  │
└───────────────────────────────────────────────────────────────┘
```

### Zero-GC Frame Memory

Persistent simulation state uses flat value data (fixed arrays, structs, `Pool[T]`). Transient frame data is allocated in regional bump arenas (`arena { ... }`) and reclaimed in one bump-pointer reset:

```ube
while game.is_running {
    arena {
        game.begin_frame()
        game.draw_rect(player.x, player.y, 32.0, 32.0, color::green())
        game.present()
    } // Transient scratch memory wiped in 1 machine cycle
}
```

---

## How to Run Examples

All examples are located under `examples/` and can be checked, compiled to WASI 0.2 components, and run with the host runner:

```bash
cd _groot

# 1. Type-check any example
../bin/ube check examples/01_bunnymark/main.ube

# 2. Compile to a WASI 0.2 WebAssembly Component (.wasm)
../bin/ube build examples/01_bunnymark/main.ube --target=wasi2 -o bunnymark.wasm

# 3. Run with the native host runner
cargo run --manifest-path host/Cargo.toml -- bunnymark.wasm
```

### Quick Run Matrix

| Example | Build Command | Run Command |
| :--- | :--- | :--- |
| **01 Bunnymark** | `../bin/ube build examples/01_bunnymark/main.ube --target=wasi2 -o bunnymark.wasm` | `cargo run --manifest-path host/Cargo.toml -- bunnymark.wasm` |
| **02 Platformer** | `../bin/ube build examples/02_platformer/main.ube --target=wasi2 -o platformer.wasm` | `cargo run --manifest-path host/Cargo.toml -- platformer.wasm` |
| **03 Physics Sandbox** | `../bin/ube build examples/03_physics_sandbox/main.ube --target=wasi2 -o physics.wasm` | `cargo run --manifest-path host/Cargo.toml -- physics.wasm` |
| **04 Generational ECS** | `../bin/ube build examples/04_generational_ecs/main.ube --target=wasi2 -o ecs.wasm` | `cargo run --manifest-path host/Cargo.toml -- ecs.wasm` |
| **05 Camera & Coords** | `../bin/ube build examples/05_camera_and_coords/main.ube --target=wasi2 -o camera.wasm` | `cargo run --manifest-path host/Cargo.toml -- camera.wasm` |
| **Flappy Groot** | `../bin/ube build examples/flappy_groot/main.ube --target=wasi2 -o flappy.wasm` | `cargo run --manifest-path host/Cargo.toml -- flappy.wasm` |

---

## Directory Layout

```
_groot/
├── Cargo.toml                 # Rust workspace
├── wit/
│   ├── groot.wit              # ube:graphics window/gpu world
│   └── deps/                  # vendored WASI 0.2 WIT packages
├── host/                      # Native runner (winit + wgpu + wasmtime)
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs            # component instantiation + event loop
│       ├── window.rs          # winit window manager
│       ├── gpu.rs             # wgpu sprite pipeline / vertex batches
│       └── input.rs           # raw host input queue
├── pkgs/groot/                # Core Ube Engine Packages
│   ├── app/                   # Game lifecycle and engine harness
│   ├── math/                  # Vec2, Vec3, Mat4, Rect, Transform
│   ├── color/                 # RGBA colors
│   ├── render/                # Camera2D, SpriteBatcher (8-float vertex layout)
│   ├── input/                 # Typed key/mouse state and InputManager
│   ├── ecs/                   # Pool[Entity] world with generational handles
│   ├── physics2d/             # AABB manifolds, raycasts, MTV resolution
│   ├── assets/                # Texture-atlas regions and slicing
│   ├── time/                  # Delta timing and FPS tracking
│   └── platform/              # Guest bridge to ube:graphics WASI imports
├── examples/                  # Fully working, documented examples
│   ├── 01_bunnymark/          # 1,000 bouncing entities stress test
│   ├── 02_platformer/         # Gravity, jump physics, and AABB collision
│   ├── 03_physics_sandbox/    # Multi-body momentum & impulse resolution
│   ├── 04_generational_ecs/   # Generational handle safety with Pool[Entity]
│   ├── 05_camera_and_coords/  # World-to-screen transforms, pan, and zoom
│   └── flappy_groot/          # Full Flappy Bird game sample
├── docs/                      # Comprehensive engine documentation
│   ├── architecture/          # System design, Zero-GC, Host ABI
│   └── guides/                # Getting started, running examples, 2D batching, ECS
├── llm.txt                    # Consolidated reference for LLM coding agents
└── legacy/                    # Previous GoScript engine (historical reference)
```

---

## Learning Groot

- Read [`docs/README.md`](docs/README.md) for full tutorials and guides.
- Read [`docs/guides/01-getting-started.md`](docs/guides/01-getting-started.md) for detailed compilation flags and runner configuration.
- Read [`llm.txt`](llm.txt) for an LLM-optimized single-document reference.
- Explore each folder under [`examples/`](examples/) with dedicated README guides explaining core game development principles.
