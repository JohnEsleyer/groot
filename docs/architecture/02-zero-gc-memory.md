# 02 — Zero-GC Memory Management

Memory allocation in Groot is structured into three tiers:

## Tier 1: Persistent Value Structs
All persistent simulation data (rigidbodies, transform coordinates, vectors, colors) are plain value types allocated in contiguous arrays or stack records:
- No pointer indirection.
- Cache-line friendly layouts.

## Tier 2: Regional Frame Scratch Arenas (`arena { ... }`)
Intermediate frame mathematics (spatial partitions, raycast buffers, rendering vertex arrays) are executed within bump arenas:

```ube
while game.is_running {
    // Scratch arena allocated on stack/bump pointer
    arena {
        game.begin_frame()
        game.draw_rect(x, y, w, h, color::red())
        game.present()
    } // Entire scratch block reclaimed instantly in 1 cycle!
}
```

## Tier 3: Generational Entity Pools (`Pool[T]`)
Dynamic game entities (projectiles, enemies, particles) live in `Pool[T]`:
- Generational handles (`UbeHandle`) protect against stale indexing.
- Despawned slots are recycled immediately without heap reallocations.
