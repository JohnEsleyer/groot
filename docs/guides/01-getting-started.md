# Guide 01 — Getting Started & Project Scaffolding

## 1. Creating a New Game

Create your game entrypoint `my_game/main.ube`:

```ube
package main

import "../../pkgs/groot/app" as app
import "../../pkgs/groot/color" as color

fn main() {
    let mut game = app::app_new(app::EngineConfig{
        width: 800,
        height: 600,
        title: "My First Groot Game"
    })
    game.start()

    let mut frame = 0
    while game.is_running && frame < 300 {
        arena {
            game.begin_frame()
            game.draw_rect(100.0, 100.0, 64.0, 64.0, color::green())
            game.present()
        }
        frame += 1
    }
}
```

## 2. Compiling to WebAssembly

Use the Ube compiler toolchain targeting WASI 0.2:

```bash
ube check my_game/main.ube
ube build my_game/main.ube --target=wasi2 -o game.wasm
```

## 3. Running with Native Runner

Launch the game with the Groot native host:

```bash
cargo run --release --bin groot-host -- game.wasm
```
