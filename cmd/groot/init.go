// File: cmd/groot/init.go
// Implements `groot init [dir]` — scaffold a new Groot project.
package main

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
)

func runInit(args []string) error {
	dir := "."
	template := "2d"

	for i := 0; i < len(args); i++ {
		arg := args[i]
		if strings.HasPrefix(arg, "--template=") {
			template = strings.TrimPrefix(arg, "--template=")
		} else if !strings.HasPrefix(arg, "-") {
			dir = arg
		}
	}

	abs, err := filepath.Abs(dir)
	if err != nil {
		return err
	}
	name := filepath.Base(abs)
	if name == "." || name == "/" {
		name = "my_game"
	}

	// Create directory structure.
	dirs := []string{abs, filepath.Join(abs, "src"), filepath.Join(abs, "assets")}
	for _, d := range dirs {
		if err := os.MkdirAll(d, 0755); err != nil {
			return fmt.Errorf("creating directory %s: %w", d, err)
		}
	}

	manifestPath := filepath.Join(abs, "groot.toml")
	if _, err := os.Stat(manifestPath); err == nil {
		return fmt.Errorf("groot.toml already exists in %s", abs)
	}

	manifest := fmt.Sprintf(`[project]
name = "%s"
version = "0.1.0"
title = "%s"

[window]
width = 1280
height = 720
resizable = true

[render]
fps = 60
vsync = true
clear_color = [0.06, 0.06, 0.09, 1.0]

[build]
entry = "src/main.ube"
target = "component"
`, name, titleCase(name))

	if err := os.WriteFile(manifestPath, []byte(manifest), 0644); err != nil {
		return fmt.Errorf("writing groot.mod: %w", err)
	}

	mainSource := templateSource(name, template)
	mainPath := filepath.Join(abs, "src", "main.ube")
	if _, err := os.Stat(mainPath); os.IsNotExist(err) {
		if err := os.WriteFile(mainPath, []byte(mainSource), 0644); err != nil {
			return fmt.Errorf("writing src/main.ube: %w", err)
		}
	}

	gitignore := filepath.Join(abs, ".gitignore")
	if _, err := os.Stat(gitignore); os.IsNotExist(err) {
		_ = os.WriteFile(gitignore, []byte("*.wasm\n"), 0644)
	}

	fmt.Printf("%s %s %s\n",
		paint("◆", colorPurpleBold),
		paint("Initialized", colorPurple),
		paint(fmt.Sprintf("Groot project '%s' in %s", name, abs), colorDim),
	)
	fmt.Printf("\n  cd %s\n  groot start .\n\n", dir)
	return nil
}

func titleCase(s string) string {
	s = strings.ReplaceAll(s, "_", " ")
	words := strings.Fields(s)
	for i, w := range words {
		if len(w) > 0 {
			words[i] = strings.ToUpper(w[:1]) + w[1:]
		}
	}
	return strings.Join(words, " ")
}

func templateSource(name, tmpl string) string {
	switch tmpl {
	case "empty":
		return fmt.Sprintf(`package main

import "github.com/JohnEsleyer/groot/app"

fn main() {
    let mut game = app::app_new_from_manifest()
    game.start()

    while game.is_running {
        game.begin_frame()
        arena {
            game.present()
        }
    }
}
`)
	case "3d":
		return fmt.Sprintf(`package main

import "github.com/JohnEsleyer/groot/app"
import "github.com/JohnEsleyer/groot/color"
import "github.com/JohnEsleyer/groot/math"

fn main() {
    let mut game = app::app_new_from_manifest()
    game.start()

    while game.is_running {
        game.begin_frame()
        arena {
            // 3D rendering goes here
            game.present()
        }
    }
}
`)
	default: // "2d"
		return fmt.Sprintf(`package main

import "github.com/JohnEsleyer/groot/app"
import "github.com/JohnEsleyer/groot/color"
import "github.com/JohnEsleyer/groot/input"

fn main() {
    // Reads window title, size, and FPS from ube.mod / groot.toml automatically.
    let mut game = app::app_new_from_manifest()
    game.start()

    let x: f32 = 100.0
    let y: f32 = 100.0

    while game.is_running {
        game.begin_frame()
        if game.input.is_key_pressed(input::key_escape()) {
            game.stop()
        }

        // All draw calls and allocations inside 'arena' are reclaimed each frame.
        arena {
            game.draw_rect(x, y, 64.0, 64.0, color::purple())
            game.present()
        }
    }
}
`)
	}
}
