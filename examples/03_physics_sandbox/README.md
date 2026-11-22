# Example 03 — Physics Sandbox

## Core Concept: Dynamic Particle Collisions and Impulse Resolution

Demonstrates continuous boundary reflection, dynamic sphere/box AABB intersections, and momentum-preserving velocity swaps.

### Key Principles

1. **Restitution & Impulse Response**:
   - Computes relative velocities across contact normals `(v1 - v0) · n` and applies equal and opposite impulse impulses.
2. **Double-Buffering & Zero-Allocation Loops**:
   - Particle updates execute directly over flat arrays with no heap allocations during simulation ticks.

---

## How to Run

```bash
# From the _groot directory (using the project ube compiler):
../bin/ube build examples/03_physics_sandbox/main.ube --target=wasi2 -o physics.wasm
cargo run --manifest-path host/Cargo.toml -- physics.wasm
```
