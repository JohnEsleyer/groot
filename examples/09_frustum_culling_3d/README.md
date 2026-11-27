# Example 09 — Dynamic Frustum Culling (3D)

## Core Concept: Submit Only What the Camera Can See

The scene contains **400 pillar entities** in a 20×20 grid, but only the subset
inside the camera's view frustum is sent to the GPU each frame. This example
shows how to turn `math::Frustum` and `math::AABB3D` into a practical per-frame
visibility pass.

### Key Principles

1. **Frustum Extraction**:
   - `cam.frustum()` derives six normalized clipping planes directly from the
     camera's view-projection matrix.
2. **AABB vs. Frustum Test**:
   - Each pillar stores a world-space `AABB3D`. Calling
     `frustum.intersects_aabb(bounds)` uses the positive-vertex test against all
     six planes and returns `false` as soon as one plane rejects the box.
3. **Reusable World-Space Data**:
   - `Pillar` stores `bounds`, `model`, and `tint` once at startup. Per-frame
     work is limited to testing bounds and pushing visible entries into scratch
     slices.
4. **Visible-Only Instanced Draw**:
   - `visible_models` / `visible_tints` contain only visible pillars, so
     `draw_instances_tinted` receives a smaller instance buffer as the camera
     turns away from parts of the grid.
5. **Regional Arena**:
   - The visible lists and ground transform are allocated inside `arena { ... }`
     and discarded at the end of the frame.

### Scene Setup

| Item | Value |
| :--- | :--- |
| Pillars | 20 × 20 = 400 |
| Spacing | 5 units |
| Pillar size | 2 × variable height × 2 |
| Camera | Circular sweep at radius 55 |
| FOV | 40° (narrow, to make culling obvious) |
| Far plane | 150 units |

A console line is printed every 60 frames:

```text
[frame 0] Visible: N/400, Culled: M
```

Watch the visible count change as the camera circles the grid.

---

## How to Run

```bash
# From the _groot directory:
../bin/ube build examples/09_frustum_culling_3d/main.ube --target=wasi2 -o culling3d.wasm
cargo run --manifest-path host/Cargo.toml -- culling3d.wasm
```

Or use the helper script:

```bash
./run.sh 09_frustum_culling_3d
```

### Controls

| Input | Action |
| :--- | :--- |
| — | Camera sweeps around the grid automatically; close the window to exit |

### Things to Try

- Lower `fov_y` further to see fewer pillars survive the test.
- Move the `far` plane in from `150.0` to cull distant pillars as well.
- Replace the AABB with a tighter box to see how bounds quality affects
  conservative culling.
