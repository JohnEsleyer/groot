# Example 05 — Camera and Coordinate Transforms

## Core Concept: Orthographic Projections, Pan, Zoom, and Screen Mapping

Demonstrates how to decouple gameplay coordinates from screen pixel coordinates using `Camera2D`.

### Key Principles

1. **World vs. Screen Coordinates**:
   - `world_to_screen(world_pos)` centers the viewport, adjusts for zoom scale, and translates camera position into window coordinates.
2. **Camera Matrices & Viewports**:
   - Decoupled from native window metrics; viewport transformations easily support letterboxing and multi-resolution rendering.

---

## How to Run

```bash
# From the _groot directory (using the project ube compiler):
ube build examples/05_camera_and_coords/main.ube --target=wasi2 -o camera.wasm
cargo run --manifest-path host/Cargo.toml -- camera.wasm
```
