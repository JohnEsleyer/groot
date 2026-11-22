# Groot Engine Examples

This directory contains complete, runnable examples demonstrating the 2D and 3D capabilities of the Groot game engine in Ube.

All examples compile to **WASI 0.2 WebAssembly Components** and run on the native host (`winit` + `wgpu` + `wasmtime`).

---

## Prerequisites

1. **Rust toolchain & Cargo**: Ensure `cargo` and `rustc` are available on your system.
2. **Project Ube Compiler**: Use the compiler binary located at `bin/ube` relative to the repository root (or `../bin/ube` from inside `_groot`).
   > **Note:** If you have an older global `ube` installed (e.g. in `/usr/local/bin/ube`), it may lack recent WASI 0.2 component bindings (such as `ube:graphics/gpu3d@0.2.0`). Always invoke `../bin/ube` from `_groot/`.

---

## Quick Run Helper (`./run.sh`)

A helper script [`./run.sh`](run.sh) is provided in `_groot` to build and run any example with a single short command without dealing with multiline paste errors or path flags:

```bash
cd _groot

# Run the 2D platformer (default if no argument provided)
./run.sh 02_platformer

# Or run any other example by folder name:
./run.sh 01_bunnymark
./run.sh 03_physics_sandbox
./run.sh 04_generational_ecs
./run.sh 05_camera_and_coords
./run.sh 06_cubes_3d
./run.sh 07_host_tick_3d
./run.sh flappy_groot
```

| Directory | Name | Type | Description |
| :--- | :--- | :--- | :--- |
| [`01_bunnymark/`](01_bunnymark/README.md) | **Bunnymark Benchmark** | 2D | 1,000 bouncing sprites stress-testing batch vertex submission and regional bump arenas. |
| [`02_platformer/`](02_platformer/README.md) | **2D Platformer** | 2D | Kinematics, gravity, jump curves, input management, and AABB collision manifolds. |
| [`03_physics_sandbox/`](03_physics_sandbox/README.md) | **Physics Sandbox** | 2D | Dynamic particle and box collisions, impulse and momentum resolution. |
| [`04_generational_ecs/`](04_generational_ecs/README.md) | **Generational ECS** | 2D | Safe entity handles, slot reuse, and stale lookup safety without garbage collection. |
| [`05_camera_and_coords/`](05_camera_and_coords/README.md) | **Camera & Coordinates** | 2D | Orthographic 2D camera, world-to-screen transforms, pan, and zoom. |
| [`06_cubes_3d/`](06_cubes_3d/README.md) | **3D Instanced Cubes** | 3D | Orbiting 3D camera, directional lighting, mesh primitives, and instanced draw calls. |
| [`07_host_tick_3d/`](07_host_tick_3d/README.md) | **Host-Driven Tick (3D)** | 3D | Inverted game loop: guest exports `init()` and `tick(dt: f32) -> bool` driven by host events. |
| [`flappy_groot/`](flappy_groot/README.md) | **Flappy Groot** | 2D | Full Flappy Bird game loop with entity recycling, flap physics, and regional scratch rendering. |

---

## How to Build and Run

Run these commands from the `_groot` directory:

### 1. 01 Bunnymark
```bash
../bin/ube build examples/01_bunnymark/main.ube --target=wasi2 -o bunnymark.wasm
cargo run --manifest-path host/Cargo.toml -- bunnymark.wasm
```

### 2. 02 Platformer
```bash
../bin/ube build examples/02_platformer/main.ube --target=wasi2 -o platformer.wasm
cargo run --manifest-path host/Cargo.toml -- platformer.wasm
```

### 3. 03 Physics Sandbox
```bash
../bin/ube build examples/03_physics_sandbox/main.ube --target=wasi2 -o physics.wasm
cargo run --manifest-path host/Cargo.toml -- physics.wasm
```

### 4. 04 Generational ECS
```bash
../bin/ube build examples/04_generational_ecs/main.ube --target=wasi2 -o ecs.wasm
cargo run --manifest-path host/Cargo.toml -- ecs.wasm
```

### 5. 05 Camera & Coordinates
```bash
../bin/ube build examples/05_camera_and_coords/main.ube --target=wasi2 -o camera.wasm
cargo run --manifest-path host/Cargo.toml -- camera.wasm
```

### 6. 06 3D Instanced Cubes
```bash
../bin/ube build examples/06_cubes_3d/main.ube --target=wasi2 -o cubes3d.wasm
cargo run --manifest-path host/Cargo.toml -- cubes3d.wasm
```

### 7. 07 Host-Driven Tick (3D)
```bash
../bin/ube build examples/07_host_tick_3d/main.ube --target=wasi2 -o host_tick3d.wasm
cargo run --manifest-path host/Cargo.toml -- host_tick3d.wasm
```

### 8. Flappy Groot
```bash
../bin/ube build examples/flappy_groot/main.ube --target=wasi2 -o flappy.wasm
cargo run --manifest-path host/Cargo.toml -- flappy.wasm
```

---

## One-Line Verification Script

To build and verify compilation for all examples at once:

```bash
cd _groot
for ex in examples/*; do
  if [ -d "$ex" ]; then
    echo "Compiling $ex..."
    ../bin/ube build "$ex/main.ube" --target=wasi2 -o /dev/null
  fi
done
```
