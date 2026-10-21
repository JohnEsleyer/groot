# 05 — The `groot.*` Host API

GoScript is a sandboxed language: it cannot touch the filesystem, the GPU or
the ECS directly. Instead, the engine registers native functions under the
`groot.` package. A script calls them like any other function.

```go
func OnUpdate(dt float64) {
    var pos = groot.GetSelfPosition()
    groot.SetSelfPosition(pos[0] + 1.0*dt, pos[1], pos[2])
}
```

Everything in this API is **immediate**: the call reads or writes the state of
the entity that is currently being ticked. There is no deferred command for
transform changes — the engine reads the values back after the script returns.

## Self context

Every script runs "as" an entity. The engine sets the current entity before
calling the script and restores the world state afterwards.

| Function | Returns | Description |
| --- | --- | --- |
| `groot.GetSelfEntity() -> int` | id | The engine id of the entity running this script. |
| `groot.GetSelfPosition() -> []float64` | `[x, y, z]` | Current world position. |
| `groot.SetSelfPosition(x, y, z?)` | — | Moves the entity. `z` is optional (2D games can omit it). |
| `groot.SetSelfRotation3D(pitch, yaw, roll)` | — | Sets the Euler rotation. |
| `groot.SetSelfScale(sx, sy, sz)` | — | Sets the scale. |
| `groot.SetSelfColor(r, g, b, a)` | — | Sets the tint (0..1 per channel). |
| `groot.SetSelfCollider(w, h, z?)` | — | Replaces the collider: `Box2D` when `z` is omitted, `Box3D` otherwise. |
| `groot.SetSelfText(text)` | — | Sets the text of a `VisualText` entity. |
| `groot.DestroySelf()` | — | Marks the entity for despawn at the end of the frame. |

`GetSelfPosition` returns a **slice** — index it like any other:

```go
var pos = groot.GetSelfPosition()
var x = pos[0]
```

## Other entities

| Function | Returns | Description |
| --- | --- | --- |
| `groot.GetEntityPosition(id) -> []float64` | `[x, y, z]` | World position of another entity. |
| `groot.GetTagPositions(tag) -> [][]float64` | list | Positions of every entity carrying the tag. |

Entity ids come from the ECS component (`GoScriptComponent.entity_id`) and are
stable for the entity's lifetime. Ids `1` and `2` are used by the demo scenes
for the player and the enemy.

## Tags

| Function | Description |
| --- | --- |
| `groot.SetTextByTag(tag, text)` | Sets the text of every entity with that tag. |
| `groot.DespawnByTag(tag)` | Despawns every entity with that tag. |

Both are **deferred**: they are queued and applied after all scripts have run
for the frame, so they cannot invalidate the ECS iteration that is in progress.

## Spawning

| Function | Description |
| --- | --- |
| `groot.SpawnPrefab(path, x, y, z, tag)` | Spawns a prefab (`.prefab.ron`) at the given position. |

Spawning is deferred as well. The spawned entity gets a fresh entity id and
runs the script referenced by the prefab, if any — on its own VM instance, so
its state starts from the script's declared defaults.

```go
groot.SpawnPrefab("assets/prefabs/pipe_top.prefab.ron", 8.0, 5.0, 0.0, "Pipe")
```

## Camera

| Function | Description |
| --- | --- |
| `groot.SetCameraPosition(x, y, z)` | Moves the camera (both 2D and 3D cameras). |
| `groot.GetCameraPosition() -> []float64` | Current camera position. |
| `groot.SetCameraTarget(x, y, z)` | Sets the 3D look-at point. |
| `groot.SetCameraFov(fov)` | Sets the 3D field of view in degrees. |
| `groot.SetCameraViewport(w, h)` | Sets the 2D viewport size in world units. |

Camera changes are applied to the camera objects at the start of the next
frame.

## Input

| Function | Returns | Description |
| --- | --- | --- |
| `groot.GetAxis(axis) -> float64` | −1..1 | `"Horizontal"` or `"Vertical"`. |
| `groot.IsKeyDown(key) -> bool` | | Key is held this frame. |
| `groot.IsKeyPressed(key) -> bool` | | Key was pressed this frame. |
| `groot.IsMouseDown(button) -> bool` | | Button is held. |
| `groot.IsMousePressed(button) -> bool` | | Button was pressed this frame. |

Key names are engine-defined strings (`"KeyA"`, `"Space"`, …). Axis values are
`−1`, `0` or `1` — the engine does not blend analogue input.

## Logging

| Function | Description |
| --- | --- |
| `groot.Log(...)` | Informational message. |
| `groot.Warn(...)` | Warning. |
| `groot.Error(...)` | Error (written to stderr). |

Arguments are converted with the standard display rules, so
`groot.Log("hp:", hp)` prints `hp: 100`.

## Math and collision (pure)

These are stateless helpers — they read no entity state, so they can be used
anywhere, including outside a frame.

| Function | Returns | Description |
| --- | --- | --- |
| `groot.GetDistance2D(x1, y1, x2, y2) -> float64` | distance | Distance between two 2D points. |
| `groot.Clamp(v, min, max) -> float64` | clamped value | Constrain a value. |
| `groot.Lerp(a, b, t) -> float64` | interpolated | Linear interpolation, `t` in 0..1. |
| `groot.RectsOverlap(x1, y1, w1, h1, x2, y2, w2, h2) -> bool` | | Axis-aligned rectangle overlap. |
| `groot.CirclesOverlap(x1, y1, r1, x2, y2, r2) -> bool` | | Circle overlap. |
| `groot.CircleHitsRect(cx, cy, r, rx, ry, rw, rh) -> bool` | | Circle vs. rectangle overlap. |

## Game state

| Function | Description |
| --- | --- |
| `groot.SetGameOver(on)` | Sets the global game-over flag. |
| `groot.IsGameOver() -> bool` | Reads it. |

The flag is a process-wide switch the demo scripts use to stop gameplay.

## A complete script

```go
// assets/scripts/enemy.gos
type Enemy struct {
    PatrolSpeed float64
    Timer       float64
    PatrolRange float64
}

var enemy = Enemy{
    PatrolSpeed: 2.0,
    Timer:       0.0,
    PatrolRange: 220.0,
}

func OnUpdate(dt float64) {
    var selfId = groot.GetSelfEntity()
    var pos = groot.GetSelfPosition()

    enemy.Timer = enemy.Timer + dt
    var newX = math.Sin(enemy.Timer * enemy.PatrolSpeed) * enemy.PatrolRange
    groot.SetSelfPosition(newX, pos[1])

    var dist = groot.GetDistance2D(pos[0], pos[1], 0.0, 0.0)
    if dist < 120.0 {
        groot.Warn(fmt.Sprintf("Player detected! Distance: %.1f", dist))
    }
}
```

Note that `enemy` is a **top-level** variable: every entity running this file
has its own copy, because each entity gets its own VM instance. See
[`../architecture/04-script-runtime.md`](../architecture/04-script-runtime.md).
