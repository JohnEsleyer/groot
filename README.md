# Groot

Groot is a fast-iteration 2D game engine pairing **Bevy** (Rust ECS + rendering)
with **GoScript** — an embeddable Go-syntax scripting VM written in pure Rust.
Game logic lives in `.gs` files under `assets/scripts/` and hot-reloads on save;
the host engine is fully ECS-driven, so script state routes through Bevy
components rather than global mutable maps.

## Architecture

```
assets/scripts/*.gs  ──►  GoScript VM (HotReloadEngine)  ──►  Bevy ECS
                              ▲                                   │
                              └── groot.* host bindings ──────────┘
```

- `src/main.rs` — Bevy app bootstrap + demo scene (Flappy Bird).
- `src/groot_plugin.rs` — the engine: components (`GoScriptComponent`,
  `ScriptTransform`, `ScriptColor`, `Bird`, `PipeIndex`, `ScoreText`), the
  script host (`GrootScriptHost`), input sync, per-frame script execution, and
  cross-boundary queues for spawns/events/gizmos.
- `src/groot_module.rs` — stateless `groot.*` utilities (math, collision, log).
- `src/bin/cli.rs` — the `groot-cli` command-line tool.

Script state is **ECS-first**: `script_execution_system` copies each script
entity's `Transform`/`Sprite` into a thread-local scratch, runs `OnUpdate`,
then writes the scratch back to the components (and mirrors it into
`ScriptTransform`/`ScriptColor` for component readers). No global
`ENTITY_STATES` map.

## CLI

```bash
# Scaffold a new project folder (groot.toml + assets/scripts/player.gs)
cargo run --bin groot-cli -- new my-game

# Run the current Groot game
cargo run --bin groot-cli -- run

# Build a release bundle
cargo run --bin groot-cli -- build

# Engine info / help
cargo run --bin groot-cli -- info
```

## Run the demo

```bash
cargo run
```

A Flappy Bird demo starts immediately. Scripts in `assets/scripts/` hot-reload
when you save them; live global values are preserved across reloads.

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
