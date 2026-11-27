# Example 06 — 3D Instanced Cubes

## Core Concept: Instanced 3D Rendering with an Orbiting Camera

Demonstrates the Groot 3D pipeline: loading built-in mesh primitives, building
per-frame instance matrices inside a regional arena, and submitting them in a
single GPU draw call.

### Key Principles

1. **Instanced Draw Calls**:
   - `platform3d::draw_instances(mesh_id, transforms)` submits all 100 cube
     model matrices in one GPU call. No CPU-side looping reaches the host per
     cube.
2. **Regional Frame Arenas (`arena { ... }`)**:
   - Instance transform slices are assembled inside the arena and discarded
     automatically at the end of each frame — zero persistent heap growth.
3. **Orbiting Camera**:
   - The `Camera3D` view-projection matrix is rebuilt every frame from an
     updated eye position, demonstrating smooth camera animation with no extra
     allocations.
4. **Ground Plane**:
   - A `load_plane_mesh` primitive is instanced once per frame as the ground
     reference, showing how to mix multiple mesh types in a single pass.

---

## How to Run

```bash
# From the _groot directory:
ube build examples/06_cubes_3d/main.ube --target=wasi2 -o cubes3d.wasm
cargo run --manifest-path host/Cargo.toml -- cubes3d.wasm
```

### Controls

| Input | Action |
| :--- | :--- |
| — | Camera orbits automatically; close the window to exit |
