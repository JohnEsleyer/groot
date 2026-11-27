# Example 10 — Procedural Mesh & Quaternion Tumble (3D)

## Core Concept: Building Vertex and Index Data in Pure Ube

Instead of loading a built-in primitive, this example constructs an
**octahedron** (two four-sided pyramids joined at an equatorial ring) directly in
Ube code, uploads it with `platform3d::upload_mesh()`, and animates it with
quaternions.

### Key Principles

1. **Custom Vertex Buffer Layout**:
   - Each vertex is 8 floats: `position (x, y, z)`, `normal (x, y, z)`, and
     `uv (u, v)` — matching the built-in meshes and the `gpu3d` pipeline.
2. **Per-Face Normals**:
   - `tri_data()` computes the triangle edges, takes their cross product, and
     normalizes it. Every triangle therefore gets a flat, correctly oriented
     normal for faceted lighting.
3. **Procedural Topology**:
   - The equatorial ring is generated with `sin`/`cos`, and each of the four
     segments emits one top face and one bottom face. This produces 8 triangles
     and 24 vertices with no shared-vertex normal smoothing.
4. **Index Buffer Upload**:
   - A simple `0..24` index list is uploaded alongside the vertices via
     `upload_mesh`, showing the full path from CPU-side arrays to a GPU mesh
     handle.
5. **Quaternion Composition**:
   - Two axis-angle rotations (yaw and pitch) are converted to matrices and
     multiplied to create a continuous tumble without Euler-angle artifacts.
6. **Dynamic Light + Arena Cleanup**:
   - The directional light orbits each frame, and all per-frame instance data
     lives inside the regional `arena`.

### Mesh Data

| Property | Value |
| :--- | :--- |
| Shape | Octahedron / double pyramid |
| Radius | 2.5 |
| Triangles | 8 |
| Vertices | 24 (3 per face) |
| Floats per vertex | 8 (pos + normal + uv) |
| Index type | `u32` |

---

## How to Run

```bash
# From the _groot directory:
../bin/ube build examples/10_procedural_mesh_3d/main.ube --target=wasi2 -o procmesh3d.wasm
cargo run --manifest-path host/Cargo.toml -- procmesh3d.wasm
```

Or use the helper script:

```bash
./run.sh 10_procedural_mesh_3d
```

### Controls

| Input | Action |
| :--- | :--- |
| — | The central gem tumbles and four smaller gems orbit; close the window to exit |

### Things to Try

- Change the radius passed to `build_octahedron_mesh` and watch the mesh grow.
- Increase the ring segment count from 4 to build a higher-resolution bipyramid.
- Emit averaged normals instead of per-face normals for a smooth-shaded gem.
