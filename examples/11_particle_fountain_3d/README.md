# Example 11 — Particle Fountain Simulation (3D)

## Core Concept: Simulating 300 Physics Particles in 3D

This example runs a lightweight CPU particle system: each particle has a
position, velocity, age, lifetime, and scale. Gravity, ground bounces, and
lifecycle color transitions are integrated every frame, then the particles are
rendered as instanced cubes.

### Key Principles

1. **Fixed-Step Integration**:
   - A fixed `dt = 0.016666` advances velocity with gravity and position with
     velocity. Using a fixed step keeps the simulation deterministic and easy to
     reason about.
2. **Ground Bounce with Energy Loss**:
   - When a particle reaches `y <= 0.1`, its Y velocity is reflected and scaled
     by `0.65`, while horizontal velocity is damped by `0.8`. Particles settle
     and spread naturally instead of stopping abruptly.
3. **Lifecycle Recycling**:
   - Each particle has a randomized `lifetime`. When `age >= lifetime`,
     `reset_particle()` respawns it at the fountain nozzle with new velocity,
     lifetime, and scale — no allocation or removal from the array.
4. **Staggered Spawning**:
   - Initial ages are offset by `(i % 100) * 0.02` so the fountain starts with a
     continuous stream rather than a single synchronized burst.
5. **Color Gradients**:
   - `color::lerp` maps normalized life to a two-stage gradient:
     **yellow → orange/red → dark violet**. Alpha fades in the final stage.
6. **Zero-Overhead Frame Buffers**:
   - All instance matrices and tints are built inside `arena { ... }`. The
     persistent particle array is the only long-lived state.

### Simulation Parameters

| Parameter | Value |
| :--- | :--- |
| Particles | 300 |
| Timestep | 0.016666 s |
| Gravity | 9.8 units/s² |
| Initial speed | 4.0–9.0 units/s |
| Lifetime | 1.6–2.5 s |
| Bounce restitution | 0.65 (vertical), 0.8 (horizontal) |
| Ground plane | Y = 0 |

---

## How to Run

```bash
# From the _groot directory:
../bin/ube build examples/11_particle_fountain_3d/main.ube --target=wasi2 -o particles3d.wasm
cargo run --manifest-path host/Cargo.toml -- particles3d.wasm
```

Or use the helper script:

```bash
./run.sh 11_particle_fountain_3d
```

### Controls

| Input | Action |
| :--- | :--- |
| — | Camera orbits the fountain automatically; close the window to exit |

### Things to Try

- Increase the particle count to 1000 and observe the single instanced draw call.
- Reduce gravity or bounce restitution to change the fountain's shape.
- Add a wind vector to `vel` each step for a directional spray.
