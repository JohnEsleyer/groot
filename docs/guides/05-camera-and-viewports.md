# Guide 05 — Camera & Viewport Coordinates

The `Camera2D` component decouples your game world's spatial dimensions from screen pixel resolutions.

## Creating a Camera

```ube
import "../../pkgs/groot/render" as render
import "../../pkgs/groot/math" as math

let mut cam = render::camera_new(800.0, 600.0)
cam.position = math::vec2(250.0, 100.0)
cam.zoom = 1.5
```

## Transforming Coordinates

### World to Screen (Rendering objects relative to camera view)
```ube
let world_pos = math::vec2(200.0, 150.0)
let screen_pos = cam.world_to_screen(world_pos)
```

### Screen to World (Picking objects from mouse clicks)
```ube
let mouse_screen = math::vec2(mouse_x, mouse_y)
let mouse_world = cam.screen_to_world(mouse_screen)
```
