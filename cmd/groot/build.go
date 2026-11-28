// File: cmd/groot/build.go
// Implements `groot build [dir]` — compile a Groot project to .wasm only.
package main

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"time"
)

type BuildOptions struct {
	Dir     string
	Release bool
	Output  string
	Target  string
}

func parseBuildArgs(args []string) (BuildOptions, error) {
	opts := BuildOptions{Dir: ".", Target: "component"}
	dirSet := false
	for i := 0; i < len(args); i++ {
		arg := args[i]
		switch {
		case arg == "--release":
			opts.Release = true
		case arg == "-o":
			if i+1 >= len(args) {
				return opts, fmt.Errorf("-o requires an output path")
			}
			opts.Output = args[i+1]
			i++
		case strings.HasPrefix(arg, "--target="):
			opts.Target = strings.TrimPrefix(arg, "--target=")
		case strings.HasPrefix(arg, "-"):
			return opts, fmt.Errorf("unknown flag %q", arg)
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

func runBuild(args []string) error {
	opts, err := parseBuildArgs(args)
	if err != nil {
		return err
	}

	manifest, err := LoadManifest(opts.Dir)
	if err != nil {
		return fmt.Errorf("%w\n\nHint: run 'groot init .' to create a groot.toml in this directory", err)
	}

	wasmOut := opts.Output
	if wasmOut == "" {
		wasmOut = manifest.DefaultWasmOutput()
	}

	ubeBin, err := findUbe()
	if err != nil {
		return err
	}

	entryFile := manifest.EntryFile()
	if _, err := os.Stat(entryFile); err != nil {
		fallback := filepath.Join(manifest.Dir, "main.ube")
		if _, err2 := os.Stat(fallback); err2 == nil {
			entryFile = fallback
		} else {
			return fmt.Errorf("entry file not found: %s", entryFile)
		}
	}

	compileArgs := []string{"build", entryFile, "-o", wasmOut, "--target=" + opts.Target}
	if opts.Release {
		compileArgs = append(compileArgs, "--release")
	}

	stopAnim := startSpinner("building " + manifest.Name + "...")
	start := time.Now()
	cmd := exec.Command(ubeBin, compileArgs...)
	cmd.Dir = manifest.Dir
	cmd.Env = append(os.Environ(), "GROOT_PKG_ROOT="+grootPkgRoot(manifest.Dir))
	out, err := cmd.CombinedOutput()
	stopAnim()

	if err != nil {
		return fmt.Errorf("build failed:\n%s", string(out))
	}

	elapsed := time.Since(start).Round(time.Millisecond)
	optTag := ""
	if opts.Release {
		optTag = " [-O3]"
	}
	fmt.Printf("%s %s %s\n",
		paint("◆", colorPurpleBold),
		paint("Built", colorPurple),
		paint(fmt.Sprintf("%s → %s [wasi 0.2 component]%s (%s)", manifest.Name, wasmOut, optTag, elapsed), colorDim),
	)
	return nil
}
