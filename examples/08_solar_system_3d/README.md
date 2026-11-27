# Example 08 — 3D Solar System (Hierarchical Transforms)

## Core Concept: Parent → Child Matrix Composition

This example builds a small solar system entirely from one cube mesh and a
handful of 4×4 matrices. It demonstrates how **hierarchical world transforms**
are just matrix multiplication: a child's local transform is composed with its
parent's world transform before being submitted for instanced rendering.

### Key Principles

1. **Hierarchical Scene Graph Without a Graph Library**:
   - `earth_world` is the Earth's orbital translation.
   - The Moon's local transform (an offset around Earth) is multiplied by
     `earth_world` to produce its final world matrix:
     `mat4_mul(earth_world, moon_local)`.
   - Saturn follows the same idea: its world matrix is used as the parent for
     all 32 asteroid-ring instances.
2. **Tinted Instancing**:
   - Every body is the same `load_cube_mesh()` cube. Variety comes from the
     per-instance tint slice passed to `draw_instances_tinted`, so the Sun,
     planets, Moon, and asteroids still render in a single GPU call.
3. **Quaternion Spin**:
   - The Sun's rotation is built with
     `quat_from_axis_angle(...).to_mat4()` and combined with its scale, showing
     rotation and scale composition without Euler-angle gimbal issues.
4. **Regional Frame Arena**:
   - All transient `Mat4` and `Color` slices are created inside `arena { ... }`
     and reclaimed automatically when the frame ends.

### The Bodies

| Body | Transform | Tint |
| :--- | :--- | :--- |
| Sun | Origin, spinning, scale 4.0 | Gold |
| Mercury | Orbit radius 8, fast | Slate gray |
| Earth | Orbit radius 18 | Ocean blue |
| Moon | Earth-relative orbit radius 3.5 | White |
| Mars | Orbit radius 27 | Rust red |
| Saturn | Orbit radius 38, scale 2.6 | Pale amber |
| Ring | 32 Saturn-relative asteroid instances | Dusty gray |

---

## How to Run

```bash
# From the _groot directory:
../bin/ube build examples/08_solar_system_3d/main.ube --target=wasi2 -o solar3d.wasm
cargo run --manifest-path host/Cargo.toml -- solar3d.wasm
```

Or use the helper script:

```bash
./run.sh 08_solar_system_3d
```

### Controls

| Input | Action |
| :--- | :--- |
| — | The camera orbits and the system animates automatically; close the window to exit |

### Expected Output

A slowly panning, inclined view of a golden Sun surrounded by orbiting planets.
Earth carries a small white Moon, and Saturn is surrounded by a rotating ring of
flattened asteroid cubes.
