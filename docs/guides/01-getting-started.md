# Guide 01 — Getting Started & Running Examples

This guide details how to build and execute any Groot game or example from source.

---

## Prerequisites

1. **Ube Compiler Toolchain**:
   - The `ube` binary is located at the workspace root: `../bin/ube` (or installed on your `$PATH`).
   - Run `ube version` to verify:
     ```bash
     ../bin/ube version
     # 🍠 Ube 2.0.0
     ```

2. **Rust & Cargo**:
   - Rust toolchain (`rustc`, `cargo`) 1.75+ for compiling the native `winit` + `wgpu` host.

---

## Step-by-Step: Compiling and Running Any Example

Each example in Groot is a complete, self-contained Ube program compiling to a WASI 0.2 component (`.wasm`).

### 1. Change into the `_groot` directory
```bash
cd _groot
```

### 2. Type-Check and Verify Affine Safety
You can check any example before compiling:
```bash
../bin/ube check examples/01_bunnymark/main.ube
../bin/ube check examples/02_platformer/main.ube
../bin/ube check examples/03_physics_sandbox/main.ube
../bin/ube check examples/04_generational_ecs/main.ube
../bin/ube check examples/05_camera_and_coords/main.ube
../bin/ube check examples/flappy_groot/main.ube
```

### 3. Build to a WASI 0.2 WebAssembly Component
Pass `--target=wasi2` to compile directly to a WebAssembly Component Model binary:
```bash
# Example: Bunnymark
../bin/ube build examples/01_bunnymark/main.ube --target=wasi2 -o bunnymark.wasm

# Example: 2D Platformer
../bin/ube build examples/02_platformer/main.ube --target=wasi2 -o platformer.wasm

# Example: Flappy Groot
../bin/ube build examples/flappy_groot/main.ube --target=wasi2 -o flappy.wasm
```

### 4. Run with the Native Host Runner
Launch the compiled `.wasm` file using the `groot-host` runner:
```bash
cargo run --manifest-path host/Cargo.toml -- bunnymark.wasm
```
*Tip: For maximum frame rate and release performance in benchmarks:*
```bash
cargo run --release --manifest-path host/Cargo.toml -- bunnymark.wasm
```

---

## Quick Reference: One-Liner Execution Matrix

| Example | Build Command | Run Command |
| :--- | :--- | :--- |
| **01 Bunnymark** | `../bin/ube build examples/01_bunnymark/main.ube --target=wasi2 -o bunnymark.wasm` | `cargo run --manifest-path host/Cargo.toml -- bunnymark.wasm` |
| **02 Platformer** | `../bin/ube build examples/02_platformer/main.ube --target=wasi2 -o platformer.wasm` | `cargo run --manifest-path host/Cargo.toml -- platformer.wasm` |
| **03 Physics Sandbox** | `../bin/ube build examples/03_physics_sandbox/main.ube --target=wasi2 -o physics.wasm` | `cargo run --manifest-path host/Cargo.toml -- physics.wasm` |
| **04 Generational ECS** | `../bin/ube build examples/04_generational_ecs/main.ube --target=wasi2 -o ecs.wasm` | `cargo run --manifest-path host/Cargo.toml -- ecs.wasm` |
| **05 Camera & Coords** | `../bin/ube build examples/05_camera_and_coords/main.ube --target=wasi2 -o camera.wasm` | `cargo run --manifest-path host/Cargo.toml -- camera.wasm` |
| **Flappy Groot** | `../bin/ube build examples/flappy_groot/main.ube --target=wasi2 -o flappy.wasm` | `cargo run --manifest-path host/Cargo.toml -- flappy.wasm` |
