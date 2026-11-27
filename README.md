# Groot Engine (Ube Edition)

**Groot** is a zero-GC, data-oriented **2D/3D game engine** written in **Ube**. Guest
games compile directly to **WASI 0.2 WebAssembly components** and run inside a native
Rust host built on **winit + wgpu + wasmtime**.

There is no garbage collector, no scripting VM, and no hidden per-frame heap growth:
persistent state lives in flat values and generational pools, while transient frame data
is allocated in regional bump arenas and reclaimed in one pointer reset.

The previous GoScript-based engine is preserved under `legacy/` for historical
reference.

---

## At a Glance

- **2D renderer** — color-vertex quad batching, `Camera2D`, world/screen transforms.
- **3D forward renderer** — indexed meshes, perspective camera, directional lighting,
  per-instance model matrices, and optional per-instance tints.
- **Built-in primitives** — cube and ground plane meshes for quick 3D scenes.
- **Game systems** — generational ECS, AABB collision/raycasts, input state, texture
  atlas regions, timing/FPS tracking.
- **Two loop models** — guest-driven `main()` loops, or host-driven exported
  `init()` / `tick(dt)` entry points.
- **Single sandboxed ABI** — `ube:graphics@0.2.0` over the WASI 0.2 Component Model.

---

## Architecture

```text
┌───────────────────────────────────────────────────────────────┐
│                     GAME APPLICATION (Ube)                    │
│        gameplay, scenes, animation, UI, spawn logic           │
└──────────────────────────────┬────────────────────────────────┘
                               │
┌──────────────────────────────▼────────────────────────────────┐
│                     GROOT CORE (Ube)                          │
│  app · render · render3d · math · color · ecs · physics2d     │
│  input · assets · time · platform · platform3d                │
└──────────────────────────────┬────────────────────────────────┘
                               │ WASI 0.2 component imports
┌──────────────────────────────▼────────────────────────────────┐
│                  NATIVE HOST (Rust)                           │
│  winit window/events · wgpu 2D & 3D pipelines · wasmtime+WASI │
└───────────────────────────────────────────────────────────────┘
```

The guest owns simulation, batching, cameras, and draw-list construction. The host is a
thin, high-throughput hardware abstraction layer: it owns the window, swapchain, GPU
buffers/pipelines, input queue, and frame pacing.

### Execution models

1. **Guest-driven loop** — `main()` builds and presents frames synchronously through
   `ube:graphics/*` imports.
2. **Host-driven tick** — the component exports `init()` and `tick(dt: f32) -> bool`;
   `groot-host` calls them on each redraw, keeping timing and presentation on the host.

The runner auto-detects exported `tick`/`init` functions and falls back to the
synchronous `wasi:cli/run` path otherwise.

### Zero-GC frame memory

```ube
while game.is_running {
    arena {
        game.begin_frame()
        game.draw_rect(player.x, player.y, 32.0, 32.0, color::green())
        game.present()
    } // transient scratch memory wiped in one bump-pointer reset
}
```

---

## Quick Start

### Prerequisites

- **Rust / Cargo** — builds the native host.
- **Ube 2.0 compiler** — use the bundled `../bin/ube` from inside `_groot`, or a
  compatible `ube` on `PATH`.

### Run an example

```bash
cd _groot

# Build and run any example by folder name (defaults to 02_platformer)
./run.sh 06_cubes_3d
```

Or build and run manually:

```bash
ube build examples/06_cubes_3d/main.ube --target=wasi2 -o cubes3d.wasm
cargo run --manifest-path host/Cargo.toml -- cubes3d.wasm
```

Set a target frame rate for the host with `GROOT_FPS`:

```bash
GROOT_FPS=120 cargo run --manifest-path host/Cargo.toml -- cubes3d.wasm
```

Type-check a guest without producing a binary:

```bash
ube check examples/02_platformer/main.ube
```

---

## Examples

All examples live under `examples/`, compile to WASI 0.2 components, and run on the
native host.

| Example | Type | What it demonstrates |
| :--- | :--- | :--- |
| [`01_bunnymark`](examples/01_bunnymark/README.md) | 2D | 1,000 bouncing entities stressing quad batching and frame arenas. |
| [`02_platformer`](examples/02_platformer/README.md) | 2D | Gravity, jump curves, input handling, and AABB collision response. |
| [`03_physics_sandbox`](examples/03_physics_sandbox/README.md) | 2D | Multi-body collisions, momentum, and impulse resolution. |
| [`04_generational_ecs`](examples/04_generational_ecs/README.md) | 2D | Generational handles, slot reuse, and stale-entity safety. |
| [`05_camera_and_coords`](examples/05_camera_and_coords/README.md) | 2D | Orthographic camera, pan/zoom, and world-to-screen conversion. |
| [`06_cubes_3d`](examples/06_cubes_3d/README.md) | 3D | 100 instanced cubes, orbiting camera, ground plane, directional light. |
| [`07_host_tick_3d`](examples/07_host_tick_3d/README.md) | 3D | Host-driven `init`/`tick` loop with a static instanced cube grid. |
| [`flappy_groot`](examples/flappy_groot/README.md) | 2D | Complete game loop with entity recycling, flap physics, and scratch rendering. |

See [`examples/README.md`](examples/README.md) for per-example build commands and a
one-line verification script.

---

## Engine Packages

| Package | Purpose |
| :--- | :--- |
| `pkgs/groot/app` | `EngineConfig`, `App` lifecycle, frame begin/present helpers. |
| `pkgs/groot/render` | `SpriteBatcher` (8-float vertices), `Camera2D`, quad/rect drawing. |
| `pkgs/groot/render3d` | `Camera3D` with view, projection, view-projection, and frustum. |
| `pkgs/groot/platform` | 2D guest bridge to `ube:graphics/window`, `gpu`, and `input`. |
| `pkgs/groot/platform3d` | 3D guest bridge: mesh upload, camera, light, instanced draws, primitives. |
| `pkgs/groot/math` | `Vec2`, `Vec3`, `Mat4`, `Rect`, `Transform`, `Quat`, frustum/AABB types. |
| `pkgs/groot/color` | RGBA `Color`, presets, and interpolation. |
| `pkgs/groot/ecs` | Generational entity `World`, sprite/rigid-body components, handle lookup. |
| `pkgs/groot/physics2d` | AABB checks, collision manifolds, raycasts, MTV resolution. |
| `pkgs/groot/input` | Input state, per-frame edge detection, key code helpers. |
| `pkgs/groot/assets` | Texture-atlas regions and grid slicing. |
| `pkgs/groot/time` | Delta timing and FPS tracking. |

---

## Host ABI

The contract lives in [`wit/groot.wit`](wit/groot.wit) and is exposed to guests as
`ube:graphics@0.2.0`:

| Interface | Operations |
| :--- | :--- |
| `window` | `create-window` |
| `gpu` | `setup`, `set-vertices`, `draw-frame` |
| `gpu3d` | mesh upload/destroy, camera, directional light, instanced draw, frame present |
| `input` | key/mouse down queries, mouse position |

The `groot-runner` world includes `wasi:cli/command@0.2.0`, so components can use WASI
clock, filesystem, stdio, and other standard interfaces alongside graphics.

**3D instance layout:** every instance is **20 floats** — 16 floats for a column-major
model matrix followed by 4 floats for an RGBA tint. Guest code should always emit this
canonical layout; `platform3d::draw_instances` and `draw_instances_tinted` handle packing
for you.

---

## Repository Layout

```text
_groot/
├── Cargo.toml                 # Rust workspace
├── run.sh                     # Build + run an example by name
├── wit/
│   └── groot.wit              # ube:graphics@0.2.0 / groot-runner world
├── host/                      # Native runner (winit + wgpu + wasmtime)
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs            # component instantiation, loop detection, events
│       ├── window.rs          # winit window manager
│       ├── gpu.rs             # wgpu 2D batches and 3D forward/instanced pipelines
│       └── input.rs           # raw input queue and key/mouse state
├── pkgs/groot/                # Core Ube engine packages
│   ├── app/                   # App lifecycle and 2D frame harness
│   ├── render/                # SpriteBatcher and Camera2D
│   ├── render3d/              # Camera3D and frustum helpers
│   ├── platform/              # 2D guest→host bridge
│   ├── platform3d/            # 3D guest→host bridge and mesh primitives
│   ├── math/                  # 2D/3D math and matrices
│   ├── color/                 # RGBA color helpers
│   ├── ecs/                   # Generational entity world
│   ├── physics2d/             # AABB physics and raycasts
│   ├── input/                 # Input state and key codes
│   ├── assets/                # Texture atlas regions
│   └── time/                  # Frame timing and FPS
├── examples/                  # Runnable 2D and 3D examples with guides
├── docs/                      # Architecture docs and developer guides
├── _docs/                     # Engineering audits and post-mortems
├── llm.txt                    # Condensed API reference for LLM agents
└── legacy/                    # Previous GoScript engine (historical)
```

---

## Development

```bash
cd _groot

# Check the native host
cargo check --manifest-path host/Cargo.toml

# Build every example without keeping the artifacts
for ex in examples/*/main.ube; do
    ube build "$ex" --target=wasi2 -o /tmp/"$(basename "$(dirname "$ex")")".wasm
done
```

---

## Documentation

- [`docs/README.md`](docs/README.md) — documentation index and package quick reference.
- [`docs/architecture/01-system-overview.md`](docs/architecture/01-system-overview.md) —
  guest/host boundary and architecture rationale.
- [`docs/architecture/02-zero-gc-memory.md`](docs/architecture/02-zero-gc-memory.md) —
  arenas, pools, and affine ownership.
- [`docs/architecture/03-host-abi.md`](docs/architecture/03-host-abi.md) — the graphics
  ABI and batch submission.
- [`docs/guides/01-getting-started.md`](docs/guides/01-getting-started.md) — setup,
  compilation, and runner configuration.
- [`_docs/POST_MORTEM_3D_INSTANCING.md`](_docs/POST_MORTEM_3D_INSTANCING.md) — the 3D
  instance-stride and matrix-transpose incident.
- [`llm.txt`](llm.txt) — single-file API reference for LLM-assisted development.
