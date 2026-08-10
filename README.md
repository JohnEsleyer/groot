# Groot

Groot is a fast-iteration 2D/3D game engine pairing **Bevy** (Rust ECS + rendering)
with **GoScript** — an embeddable Go-syntax scripting VM written in pure Rust.
Game logic lives in `.go` files under `assets/scripts/` and hot-reloads on save;
visuals and prefab hierarchies live in RON asset files under `assets/prefabs/` and `assets/scenes/`.

## Architecture

Groot is **data-driven**: scripts own behavior and data; the host engine owns
representation and rendering. Visuals are declared as RON prefab data.

```
assets/scenes/*.scene.ron  ──►  Bevy ECS (3D Meshes, Lights, Sprites, UI)
assets/prefabs/*.prefab.ron ──►  RonAssetWatcher (Visual Hot Reloading)
assets/scripts/*.go        ──►  GoScript VM (Logic Hot Reloading)  ──► Entity Data
                                         ▲                                  │
                                         └── groot.* host bindings ──────────┘
```

- `assets/config.ron` — project config & render settings.
- `assets/prefabs/*.prefab.ron` — 2D/3D prefabs (sprites, text, 3D PBR meshes, lights, colliders, parent-child hierarchies).
- `assets/scenes/*.scene.ron` — scene layout, environment settings, cameras, entity initializers.
- `src/main.rs` — initializes Bevy, loads `assets/config.ron`, runs `GrootPlugin`.
- `src/groot_plugin.rs` — engine core: components (`GoScriptComponent`, `ScriptTransform`, `ScriptColor`, `Collider`), dual hot reloading, ECS execution, 2D/3D gizmos.

## CLI

```bash
# Scaffold a new project folder
cargo run --bin groot -- new my-game

# Run the current Groot game
cargo run --bin groot -- run

# Build a release bundle
cargo run --bin groot -- build
```

## Run the Demo

```bash
cargo run
```

A 3D demo scene starts up featuring a player cube with child lighting controlled via WASD/Arrows and Space, a continuously rotating golden 3D cube, and a 2D HUD text overlay. Save any `.go` or `.prefab.ron` file to see live hot reloading!

## Writing scripts

```go
type Player struct { Speed float64 }
var self = Player{Speed: 5.0}

func OnUpdate(dt float64) {
    var pos = groot.GetSelfPosition()
    var moveX = groot.GetAxis("Horizontal")
    var moveZ = -groot.GetAxis("Vertical")
    groot.SetSelfPosition(pos[0] + moveX*self.Speed*dt, pos[1], pos[2] + moveZ*self.Speed*dt)

    groot.SetSelfCollider(1.0, 1.0, 1.0)
    groot.Log("Hello from Groot 3D GoScript!")
}
```

## Dependencies

- `bevy` 0.13
- `ron` 0.8
- `goscript` (git: `github.com/johnesleyer/goscript`)
