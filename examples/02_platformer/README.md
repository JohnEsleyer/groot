# Example 02 — 2D Platformer

## Core Concept: Kinematics, AABB Collision Manifolds, and Input Tracking

A classic side-view platformer physics model incorporating gravity, horizontal and vertical movement, and continuous obstacle collision resolution.

### Key Principles

1. **Discrete Collision Manifolds (`check_aabb_xy`)**:
   - `physics2d::check_aabb_xy` calculates minimum translation vectors (MTV), penetrating depths, and collision normals.
2. **Deterministic Euler Integration**:
   - Constant delta-time step (`dt = 0.016`) guarantees repeatable jumps and floor landings.
3. **Decoupled Input Feed (`InputManager`)**:
   - Inputs are buffered and queried via `is_key_pressed` or `is_key_down` without depending on platform window handles.

---

## How to Run

```bash
# From the _groot directory (using the project ube compiler):
ube build examples/02_platformer/main.ube --target=wasi2 -o platformer.wasm
cargo run --manifest-path host/Cargo.toml -- platformer.wasm
```

### Controls

| Input | Action |
| :--- | :--- |
| `Space` / `Up` / `W` | Jump |
| `Left` / `A` | Move Left |
| `Right` / `D` | Move Right |
| `Escape` | Exit |
