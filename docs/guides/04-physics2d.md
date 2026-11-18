# Guide 04 — Physics & Collision Detection

The `pkgs/groot/physics2d` package provides fast, zero-allocation 2D spatial collision queries and contact manifolds.

## AABB Collision Manifold

Query bounding box intersections and extract the minimum translation vector (MTV) normal to resolve penetration:

```ube
import "../../pkgs/groot/physics2d" as physics2d
import "../../pkgs/groot/math" as math

let hit = physics2d::check_aabb_xy(
    player.x, player.y, 24.0, 40.0,
    wall.x, wall.y, 60.0, 60.0
)

if hit.has_collision {
    // Normal points along penetration axis
    player.x -= hit.normal.x * hit.depth
    player.y -= hit.normal.y * hit.depth
}
```

## Simple AABB Overlap Checks

```ube
let box1 = math::rect(10.0, 10.0, 50.0, 50.0)
let box2 = math::rect(40.0, 40.0, 50.0, 50.0)

if physics2d::overlaps(box1, box2) {
    // Overlapping!
}
```
