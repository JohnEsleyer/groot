// Package main implements the groot CLI — the unified launcher for the
// Groot game engine. It wraps the ube compiler, the native host runner, and
// (in debug mode) the live terminal TUI inspector.
//
// Usage:
//
//	groot start [dir]    – build + run a groot.toml project (default: .)
//	groot build [dir]    – compile to .wasm only
//	groot monitor [pid]  – attach the TUI dashboard to a running game
//	groot init [dir]     – scaffold a new project
//	groot version        – print toolchain version
//	groot help           – show the command list and flags
package main

import (
	"fmt"
	"os"
	"strings"
)

const grootVersion = "0.1.0"

func main() {
	if err := execute(os.Args[1:]); err != nil {
		fmt.Fprintf(os.Stderr, "groot: %v\n", err)
		os.Exit(1)
	}
}

func execute(args []string) error {
	if len(args) == 0 {
		printHelp()
		return nil
	}
	switch args[0] {
	case "help", "-h", "--help":
		printHelp()
		return nil
	case "version", "-v", "--version":
		fmt.Printf("groot %s\n", grootVersion)
		return nil
	case "start":
		return runStart(args[1:])
	case "build":
		return runBuild(args[1:])
	case "monitor":
		return runMonitor(args[1:])
	case "init":
		return runInit(args[1:])
	default:
		// Allow `groot .` or `groot my_game/` as shorthand for `groot start`.
		if !strings.HasPrefix(args[0], "-") {
			if info, err := os.Stat(args[0]); err == nil && info.IsDir() {
				return runStart(args)
			}
		}
		return fmt.Errorf("unknown command %q (run 'groot help' for usage)", args[0])
	}
}

func printHelp() {
	fmt.Print(`groot — Groot Game Engine Launcher

Usage:
  groot <command> [options]
  groot [dir]            Shorthand for 'groot start [dir]'

Commands:
  start [dir]            Build and run a Groot project (default: .)
  build [dir]            Compile a project to a standalone .wasm component
  monitor [pid]          Attach the live TUI inspector to a running game
  init [dir]             Scaffold a new Groot project with groot.toml
  version                Print the groot toolchain version
  help                   Show this command list

Flags for 'groot start':
  --debug                Debug mode: telemetry, arena auditing, GPU validation (default)
  --release              Release mode: wasm-opt -O3, telemetry disabled
  --monitor, -m          Launch game and open TUI inspector side-by-side
  --fps=<n>              Override FPS target (default: from groot.toml or 60)
  --profile              Emit a CPU profile to groot-<pid>.prof

Flags for 'groot build':
  --release              Optimize with wasm-opt -O3
  -o <file.wasm>         Output path (default: <project-name>.wasm)
  --target=component     Compile as WASI 0.2 component (default)

Flags for 'groot init':
  --template=<name>      Project template: 2d (default), 3d, or empty

Flags for 'groot monitor':
  <pid>                  Attach to a specific running game process
                         (default: discover /tmp/groot-active.json)

Environment:
  GROOT_HOST             Override the groot-host binary path
  GROOT_VSYNC            Enable/disable VSync (true/false)

Examples:
  groot start .
  groot start . --release
  groot start . --monitor
  groot build . --release -o dist/game.wasm
  groot init my_game --template=2d
  groot monitor
`)
}
