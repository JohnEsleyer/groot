# Example 04 — Generational ECS

## Core Concept: Generational Entity Indices with `Pool[Entity]` and `UbeHandle`

Demonstrates safe entity lifecycles, slot reuse, and stale reference prevention without garbage collection.

### Key Principles

1. **Generational Handles**:
   - `world.create_sprite_rgba(...)` returns a compact generational handle containing slot index + generation counter.
2. **Safe Slot Despawn and Reuse**:
   - Despawning an entity marks the slot free and increments its generation.
   - Any stale handle referencing the previous entity safely fails lookups (`world.get(h)` returns `none`), eliminating use-after-free bugs.
3. **Contiguous Cache Coherency**:
   - Entities reside in pre-allocated arrays, avoiding fragmentation and heap allocations during runtime spawns.

---

## How to Run

```bash
# From _groot directory:
ube build examples/04_generational_ecs/main.ube --target=wasi2 -o ecs.wasm
cargo run --manifest-path host/Cargo.toml -- ecs.wasm
```
