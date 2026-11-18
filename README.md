# Groot Engine (Ube Edition)

**Groot** is a zero-GC 2D/3D game engine written directly in **Ube**, targeting
the **WASI 0.2 Component Model** and backed by a thin native Rust host
(`winit` + `wgpu` + `wasmtime`).

The previous GoScript-based engine is preserved under `legacy/` (and on the
`legacy-goscript` git branch) for reference.

## Architecture

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

### Zero-GC frame memory

Persistent simulation state uses flat value data (fixed arrays, structs,
`Pool[T]`). Transient frame data is allocated in `arena while` / `arena for`
iterations and reclaimed in one bump-pointer reset:

```ube
arena while frame < 300 {
    let mut batch = render::batcher_new(10000)
    // ... update simulation, append vertices ...
    platform::submit(batch.vertices)
    platform::present(0.06, 0.06, 0.09)
    frame += 1
}
```

## Layout

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
├── pkgs/groot/
│   ├── math/                  # Vec2, Vec3, Rect, Mat4, Transform
│   ├── color/                 # RGBA colors
│   ├── render/                # Camera2D, SpriteBatcher
│   ├── input/                 # typed key/mouse state
│   ├── ecs/                   # Pool[Entity] world
│   ├── physics2d/             # AABB manifolds, raycasts
│   ├── assets/                # texture-atlas regions
│   ├── time/                  # frame timing
│   ├── app/                   # lifecycle harness
│   ├── platform/              # guest bridge to ube:graphics imports
│   └── groot.ube              # versioned engine entry point
├── examples/
│   ├── 01_bunnymark/
│   ├── 02_platformer/
│   └── 03_physics_sandbox/
└── legacy/                    # previous GoScript engine (reference)
```

## Build & Run

Build an example as a WASI 0.2 component:

```bash
cd _groot
../bin/ube build examples/01_bunnymark --target=wasi2 -o bunnymark.wasm
```

Run it through the native host:

```bash
cargo run --release --bin groot-host -- bunnymark.wasm
```

The host creates the window when the guest calls `platform::open_window`,
uploads the last sprite batch, presents it, and keeps the window alive until it
is closed.

## Host ABI

Ube's `--target=wasi2` backend packages `extern` imports into component
interfaces. Groot uses the shipped `ube:graphics` contracts:

```wit
interface window {
    create-window: func(width: u32, height: u32, title: string);
}

interface gpu {
    setup: func(width: u32, height: u32);
    set-vertices: func(data: list<f32>);
    draw-frame: func(r: f64, g: f64, b: f64);
}
```

A vertex is eight `f32` values: `[x, y, u, v, r, g, b, a]`. Six vertices make
one quad; the host converts pixel coordinates (top-left origin) to clip space
in the vertex shader.

## Implementation Notes

- All engine math is native Ube. `sqrt` is implemented with Newton-Raphson
  because the compiler currently type-checks but does not lower the `sqrt`
  builtin on the wasm2 backend.
- Imports use the declared package names (`math::`, `color::`, …) rather than
  short aliases. The type checker canonicalizes imported type names by package
  name, so aliases break cross-package field access.
- `Pool[T]` returns value snapshots. `World.replace` demonstrates the
  copy-on-replace pattern; gameplay code that mutates every frame should store
  hot state in flat fixed arrays.
- `host/src/input.rs` captures keyboard/mouse events. Wiring them to the guest
  requires an `input` component interface in the Ube wasi2 backend; the Ube
  `InputManager` already accepts the same event kinds, so only the host ABI
  bridge remains.
