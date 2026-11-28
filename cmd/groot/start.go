// File: cmd/groot/start.go
// Implements `groot start [dir]` — compile + launch a Groot game project.
package main

import (
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
	"time"
)

// StartOptions captures everything parsed from `groot start` flags.
type StartOptions struct {
	Dir     string
	Release bool
	Monitor bool
	FPS     int
	Profile bool
}

func parseStartArgs(args []string) (StartOptions, error) {
	opts := StartOptions{Dir: "."}
	dirSet := false
	for i := 0; i < len(args); i++ {
		arg := args[i]
		switch {
		case arg == "--release":
			opts.Release = true
		case arg == "--debug":
			// explicit --debug is a no-op; debug is the default
		case arg == "--monitor" || arg == "-m":
			opts.Monitor = true
		case arg == "--profile":
			opts.Profile = true
		case strings.HasPrefix(arg, "--fps="):
			n, err := strconv.Atoi(strings.TrimPrefix(arg, "--fps="))
			if err != nil || n < 0 {
				return opts, fmt.Errorf("invalid --fps value: %s", arg)
			}
			opts.FPS = n
		case strings.HasPrefix(arg, "-"):
			return opts, fmt.Errorf("unknown flag %q (run 'groot help' for usage)", arg)
		default:
			if !dirSet {
				opts.Dir = arg
				dirSet = true
			} else {
				return opts, fmt.Errorf("unexpected argument %q", arg)
			}
		}
	}
	return opts, nil
}

func runStart(args []string) error {
	opts, err := parseStartArgs(args)
	if err != nil {
		return err
	}

	manifest, err := LoadManifest(opts.Dir)
	if err != nil {
		return fmt.Errorf("%w\n\nHint: run 'groot init .' to create a groot.toml in this directory", err)
	}

	// FPS flag overrides manifest value.
	fps := manifest.FPS
	if opts.FPS > 0 {
		fps = opts.FPS
	}

	mode := "debug"
	if opts.Release {
		mode = "release"
	}

	printBanner(manifest, mode)

	// Step 1: Compile via the ube toolchain.
	wasmPath, err := compileProject(manifest, opts.Release)
	if err != nil {
		return err
	}

	// Step 2: If --monitor flag, launch TUI in a dedicated terminal window.
	if opts.Monitor && !opts.Release {
		go func() {
			// Small delay so the host process has started and written /tmp/groot-active.json
			time.Sleep(500 * time.Millisecond)
			_ = launchMonitorWindow()
		}()
	}

	// Step 3: Launch the native Groot host runner.
	return launchHost(manifest, wasmPath, fps, mode)
}

func printBanner(m *GrootManifest, mode string) {
	modeTag := " [DEBUG]"
	if mode == "release" {
		modeTag = " [RELEASE]"
	}
	fmt.Printf("%s %s%s\n", paint("◆", colorPurpleBold), paint("groot start", colorPurple), paint(modeTag+": "+m.Title, colorDim))
}

// compileProject invokes `ube build` with the right flags and returns the path
// to the produced .wasm file.
func compileProject(m *GrootManifest, release bool) (string, error) {
	ubeBin, err := findUbe()
	if err != nil {
		return "", err
	}

	wasmOut := m.DefaultWasmOutput()
	entryFile := m.EntryFile()

	// Verify entry file exists.
	if _, err := os.Stat(entryFile); err != nil {
		// Fallback: check for a single main.ube in the manifest's directory.
		fallback := filepath.Join(m.Dir, "main.ube")
		if _, err2 := os.Stat(fallback); err2 == nil {
			entryFile = fallback
		} else {
			return "", fmt.Errorf("entry file not found: %s\n(configured via groot.toml build.entry)", entryFile)
		}
	}

	compileArgs := []string{"build", entryFile, "-o", wasmOut, "--target=component"}
	if release {
		compileArgs = append(compileArgs, "--release")
	}

	stopAnim := startSpinner("compiling " + m.Name + "...")
	compileStart := time.Now()
	cmd := exec.Command(ubeBin, compileArgs...)
	cmd.Dir = m.Dir
	// Pass GROOT_PKG_ROOT so the compiler can resolve groot/* imports.
	cmd.Env = append(os.Environ(), "GROOT_PKG_ROOT="+grootPkgRoot(m.Dir))
	out, err := cmd.CombinedOutput()
	stopAnim()

	if err != nil {
		return "", fmt.Errorf("compilation failed:\n%s", string(out))
	}

	elapsed := time.Since(compileStart).Round(time.Millisecond)
	optTag := ""
	if release {
		optTag = " [-O3]"
	}
	fmt.Printf("%s %s %s\n",
		paint("◆", colorPurpleBold),
		paint("Compiled", colorPurple),
		paint(fmt.Sprintf("%s → %s%s (%s)", m.Name, wasmOut, optTag, elapsed), colorDim),
	)

	return wasmOut, nil
}

// launchHost finds and runs the groot-host binary.
func launchHost(m *GrootManifest, wasmPath string, fps int, mode string) error {
	hostBin, err := findGrootHost(m.Dir)
	if err != nil {
		return err
	}

	env := append(os.Environ(),
		fmt.Sprintf("GROOT_FPS=%d", fps),
		fmt.Sprintf("GROOT_VSYNC=%t", m.VSync),
		fmt.Sprintf("GROOT_MODE=%s", mode),
		fmt.Sprintf("GROOT_PROJECT=%s", m.Name),
		fmt.Sprintf("GROOT_TITLE=%s", m.Title),
		fmt.Sprintf("GROOT_WIDTH=%d", m.Width),
		fmt.Sprintf("GROOT_HEIGHT=%d", m.Height),
	)
	if mode == "debug" {
		env = append(env, "RUST_LOG=info")
	}

	cmd := exec.Command(hostBin, wasmPath)
	cmd.Env = env
	cmd.Stdin = os.Stdin
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr

	if err := cmd.Start(); err != nil {
		return err
	}

	pid := cmd.Process.Pid
	session := map[string]interface{}{
		"project_name": m.Name,
		"title":        m.Title,
		"mode":         mode,
		"pid":          pid,
		"socket_path":  fmt.Sprintf("/tmp/groot-%d.sock", pid),
		"started_at":   time.Now().Unix(),
	}
	if sessionBytes, err := json.Marshal(session); err == nil {
		_ = os.WriteFile("/tmp/groot-active.json", sessionBytes, 0644)
	}
	defer func() { _ = os.Remove("/tmp/groot-active.json") }()

	if err := cmd.Wait(); err != nil {
		if e, ok := err.(*exec.ExitError); ok && e.ExitCode() == 0 {
			return nil
		}
		return err
	}
	return nil
}

// findUbe locates the `ube` binary: first on PATH, then as a sibling of groot.
func findUbe() (string, error) {
	if p, err := exec.LookPath("ube"); err == nil {
		return p, nil
	}
	// Sibling binary next to groot (common in a dist layout).
	if self, err := os.Executable(); err == nil {
		sibling := filepath.Join(filepath.Dir(self), "ube")
		if _, err := os.Stat(sibling); err == nil {
			return sibling, nil
		}
	}
	return "", fmt.Errorf("'ube' not found on PATH — install it or add it to your PATH")
}

// findGrootHost locates the groot-host binary.
//
// Search order:
//  1. GROOT_HOST env var (explicit override)
//  2. `groot-host` on PATH
//  3. `_groot/target/debug/groot-host` or `_groot/target/release/groot-host`
//     relative to the project's groot.mod directory, then ascending to find _groot/
func findGrootHost(projectDir string) (string, error) {
	if v := os.Getenv("GROOT_HOST"); v != "" {
		return v, nil
	}
	if p, err := exec.LookPath("groot-host"); err == nil {
		return p, nil
	}
	// Walk up from projectDir looking for the _groot/ directory.
	curr := projectDir
	for {
		for _, profile := range []string{"debug", "release"} {
			candidate := filepath.Join(curr, "_groot", "target", profile, "groot-host")
			if _, err := os.Stat(candidate); err == nil {
				return candidate, nil
			}
		}
		parent := filepath.Dir(curr)
		if parent == curr {
			break
		}
		curr = parent
	}
	return "", fmt.Errorf(
		"groot-host binary not found.\n" +
			"Build it with: cd _groot && cargo build\n" +
			"Or set GROOT_HOST=/path/to/groot-host",
	)
}

// grootPkgRoot returns the absolute path to the _groot/pkgs directory so the
// ube compiler can resolve groot/* imports when GROOT_PKG_ROOT is set.
func grootPkgRoot(projectDir string) string {
	curr := projectDir
	for {
		candidate := filepath.Join(curr, "_groot", "pkgs")
		if _, err := os.Stat(candidate); err == nil {
			return candidate
		}
		parent := filepath.Dir(curr)
		if parent == curr {
			break
		}
		curr = parent
	}
	return ""
}
