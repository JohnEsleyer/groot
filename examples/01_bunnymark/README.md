# Example 01 — Bunnymark Benchmark

## Core Concept: High-Performance Batch Rendering & Zero GC Overhead

This benchmark stresses the 2D quad batching pipeline by spawning and updating 1,000 bouncing entities at 60 FPS without GC stutter or per-draw FFI overhead.

### Key Principles

1. **Flat Value Structs (`Bunny`)**:
   - Simulation state (`x, y, vx, vy`) is stored in flat arrays/slices with zero pointer indirection.
2. **Transient Frame Arenas (`arena { ... }`)**:
   - Vertex accumulation buffers and scratch calculations are generated within per-frame bump arenas.
   - When the frame finishes, all transient memory resets via bump-pointer pointer rollback in a single machine cycle.
3. **Batched Geometry Submission**:
   - Quads are formatted into interleaved 8-float vertices (`x, y, u, v, r, g, b, a`) and flushed directly to the host GPU via `submit()` and `present()`.

---

## How to Run

```bash
# From _groot directory:
ube build examples/01_bunnymark/main.ube --target=wasi2 -o bunnymark.wasm
cargo run --manifest-path host/Cargo.toml -- bunnymark.wasm
```
