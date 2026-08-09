# Groot

Groot is a fast-iteration 2D game engine pairing **Bevy** (Rust ECS + rendering)
with **GoScript** — an embeddable Go-syntax scripting VM written in pure Rust.
Game logic lives in `.go` files under `assets/scripts/` and hot-reloads on save;
the host engine is fully ECS-driven, so script state routes through Bevy
components rather than global mutable maps.

## Architecture

Groot is **data-driven**: scripts own behavior and data; the host engine owns
representation and rendering. Visuals are declared as prefab data in
`groot.toml`, so no Rust is needed to define what anything looks like.

```
groot.toml (prefabs + scene)  ──►  Bevy ECS (sprites/UI from prefab DATA)
assets/scripts/*.go  ──►  GoScript VM (HotReloadEngine)  ──►  entity DATA
                                     ▲                                  │
                                     └── groot.* host bindings ──────────┘
```

- `groot.toml` — project config: `[window]`, `[[prefab]]` (sprite/text visuals +
  optional behavior script), `[[scene.entity]]` (initial entities).
- `src/main.rs` — loads `groot.toml`, builds the window, inserts `GrootConfig`.
- `src/groot_plugin.rs` — the engine: components (`GoScriptComponent`,
  `ScriptTransform`, `ScriptColor`, `Collider`, `PipeIndex`, `ScoreText`),
  the script host (`GrootScriptHost`), prefab/scene spawning, input sync,
  per-frame script execution, and cross-boundary queues for spawns/events.
- `src/groot_module.rs` — stateless `groot.*` utilities (math, collision, log).
- `src/bin/cli.rs` — the `groot` CLI tool (`groot new` / `run` / `build`).

Script state is **ECS-first**: `script_execution_system` copies each script
entity's `Transform`/`Sprite`/`Collider` into a thread-local scratch, runs
`OnUpdate`, then writes the scratch back to the components (and mirrors it into
`ScriptTransform`/`ScriptColor` for component readers). No global
`ENTITY_STATES` map. Scripts never draw; debug collider overlays are rendered
from `Collider` data by the engine when `DebugRender.show_colliders` is on.

## CLI

```bash
# Scaffold a new project folder (groot.toml + prefab + assets/scripts/player.go)
cargo run --bin groot -- new my-game

# Run the current Groot game
cargo run --bin groot -- run

# Build a release bundle
cargo run --bin groot -- build

# Engine info / help
cargo run --bin groot -- info
```

Or install the CLI globally and call it directly:

```bash
cargo install --path . --bin groot
groot new my-game
```

## Run the demo

```bash
cargo run
```

A Flappy Bird demo starts immediately. Its window, bird sprite, ground, pipes,
and score HUD are **data** in `groot.toml`; `flappy.go` only provides
behavior. Scripts in `assets/scripts/` hot-reload when you save them;
live global values are preserved across reloads.

## Writing scripts

```go
type Player struct { Speed float64 }
var self = Player{Speed: 300.0}

func OnUpdate(dt float64) {
    var pos = groot.GetSelfPosition()
    var move = groot.GetAxis("Horizontal")
    groot.SetSelfPosition(pos[0] + move*self.Speed*dt, pos[1])
    groot.Log("Hello from Groot GoScript!")
}
```

## Dependencies

- `bevy` 0.13
- `goscript` (git: `github.com/johnesleyer/goscript`)
