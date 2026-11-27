# Example 07 — Host-Driven Tick (3D)

## Core Concept: Exported `init` / `tick` Entry Points for Host-Controlled Game Loops

This example shows the _alternative_ guest API where the **host drives the loop**
rather than the guest. Instead of a `while` loop in `main`, the guest exports
`init()` and `tick(dt: f32) -> bool`. The host calls them on every frame,
giving the host full control over timing, vsync, and frame rate.

### Key Principles

1. **Exported Functions as the Guest Interface**:
   - `export fn init()` — called once after the component is instantiated.
   - `export fn tick(dt: f32) -> bool` — called every frame; return `true` to
     keep running, `false` to signal shutdown.
2. **Stateless Camera Per Tick**:
   - The camera matrices and instance lists are rebuilt from scratch inside each
     `tick` call. Combined with an `arena` block, this keeps persistent memory
     usage at zero outside the guest's globals.
3. **Host Timing Authority**:
   - The host passes the real `dt` (seconds since last frame), so the guest
     never needs to query a clock or manage sleep intervals.

> [!NOTE]
> The groot-host runner auto-detects whether a component exports `tick` or uses
> a `main`-style loop. Both patterns are fully supported.

---

## How to Run

```bash
# From the _groot directory:
ube build examples/07_host_tick_3d/main.ube --target=wasi2 -o host_tick3d.wasm
cargo run --manifest-path host/Cargo.toml -- host_tick3d.wasm
```

### Controls

| Input | Action |
| :--- | :--- |
| — | Static 5×5 cube grid; close the window to exit |
