# Guide 02 — 2D Batch Rendering Pipeline

Groot's 2D renderer utilizes a batching mechanism that groups multiple quad draw calls into a contiguous dynamic vertex stream.

## Vertex Buffer Layout
Each vertex is packed into 8 consecutive 32-bit floats (`32 bytes` total):

| Offset | Component | Type | Description |
| :--- | :--- | :--- | :--- |
| `0` | `x` | `f32` | Normalized or screen-space X coordinate |
| `1` | `y` | `f32` | Normalized or screen-space Y coordinate |
| `2` | `u` | `f32` | Texture U coordinate `[0.0, 1.0]` |
| `3` | `v` | `f32` | Texture V coordinate `[0.0, 1.0]` |
| `4` | `r` | `f32` | Red channel `[0.0, 1.0]` |
| `5` | `g` | `f32` | Green channel `[0.0, 1.0]` |
| `6` | `b` | `f32` | Blue channel `[0.0, 1.0]` |
| `7` | `a` | `f32` | Alpha channel `[0.0, 1.0]` |

Every quad produces 2 triangles (6 vertices = 48 floats).

## Using `SpriteBatcher` Directly

```ube
import "../../pkgs/groot/render" as render
import "../../pkgs/groot/color" as color
import "../../pkgs/groot/platform" as platform

let mut batch = render::batcher_new(1024)
batch.clear()

// Draw colored rectangle
batch.draw_rect(50.0, 50.0, 100.0, 100.0, color::blue())

// Submit to host and present
platform::submit(batch.vertices)
platform::present(0.1, 0.1, 0.1)
```
