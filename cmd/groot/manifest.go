// File: cmd/groot/manifest.go
// Parses groot.mod — the TOML project manifest for Groot game projects.
package main

import (
	"bufio"
	"fmt"
	"os"
	"path/filepath"
	"strconv"
	"strings"
)

// GrootManifest holds the parsed contents of a groot.mod file.
type GrootManifest struct {
	// [project]
	Name    string
	Version string
	Title   string

	// [window]
	Width     int
	Height    int
	Resizable bool

	// [render]
	FPS        int
	VSync      bool
	ClearColor [4]float64

	// [build]
	Entry  string
	Target string

	// Source directory (the directory containing groot.mod).
	Dir string
}

// FindManifest searches for groot.toml (preferred) or groot.mod starting from
// dir and walking up to the filesystem root. Returns the absolute path of the
// manifest file, or "" if not found.
//
// Search order per directory: groot.toml is preferred; groot.mod is a legacy
// fallback so old examples and projects keep working without changes.
func FindManifest(dir string) string {
	abs, err := filepath.Abs(dir)
	if err != nil {
		abs = dir
	}
	curr := abs
	for {
		// Prefer groot.toml (new canonical name).
		for _, name := range []string{"groot.toml", "groot.mod"} {
			candidate := filepath.Join(curr, name)
			if _, err := os.Stat(candidate); err == nil {
				return candidate
			}
		}
		parent := filepath.Dir(curr)
		if parent == curr {
			break
		}
		curr = parent
	}
	return ""
}

// LoadManifest parses groot.toml (or legacy groot.mod) from the given project
// directory. It walks up the filesystem if no manifest is found in dir itself.
func LoadManifest(dir string) (*GrootManifest, error) {
	manifestPath := FindManifest(dir)
	if manifestPath == "" {
		return nil, fmt.Errorf("no groot.toml found in %s or any parent directory", dir)
	}
	data, err := os.ReadFile(manifestPath)
	if err != nil {
		return nil, fmt.Errorf("reading %s: %w", filepath.Base(manifestPath), err)
	}
	m, err := parseManifest(string(data))
	if err != nil {
		return nil, fmt.Errorf("parsing %s: %w", filepath.Base(manifestPath), err)
	}
	m.Dir = filepath.Dir(manifestPath)
	// Apply defaults.
	if m.Name == "" {
		m.Name = filepath.Base(m.Dir)
	}
	if m.Title == "" {
		m.Title = m.Name
	}
	if m.Width == 0 {
		m.Width = 1280
	}
	if m.Height == 0 {
		m.Height = 720
	}
	if m.FPS == 0 {
		m.FPS = 60
	}
	if m.Entry == "" {
		m.Entry = "src/main.ube"
	}
	if m.Target == "" {
		m.Target = "component"
	}
	if m.ClearColor == ([4]float64{}) {
		m.ClearColor = [4]float64{0.06, 0.06, 0.09, 1.0}
	}
	return m, nil
}

// parseManifest is a minimal TOML-like parser that handles groot.mod format.
// It supports [section] headers and key = value pairs (string, integer, bool,
// and inline array literals).
func parseManifest(src string) (*GrootManifest, error) {
	m := &GrootManifest{}
	section := ""
	scanner := bufio.NewScanner(strings.NewReader(src))
	for scanner.Scan() {
		line := strings.TrimSpace(scanner.Text())
		if line == "" || strings.HasPrefix(line, "#") {
			continue
		}
		if strings.HasPrefix(line, "[") && strings.HasSuffix(line, "]") {
			section = strings.ToLower(strings.TrimSpace(line[1 : len(line)-1]))
			continue
		}
		eqIdx := strings.IndexByte(line, '=')
		if eqIdx < 0 {
			continue
		}
		key := strings.ToLower(strings.TrimSpace(line[:eqIdx]))
		rawVal := strings.TrimSpace(line[eqIdx+1:])

		switch section + "." + key {
		// [project]
		case "project.name", ".name":
			m.Name = unquote(rawVal)
		case "project.version", ".version":
			m.Version = unquote(rawVal)
		case "project.title", ".title":
			m.Title = unquote(rawVal)
		// [window]
		case "window.width":
			m.Width, _ = strconv.Atoi(rawVal)
		case "window.height":
			m.Height, _ = strconv.Atoi(rawVal)
		case "window.resizable":
			m.Resizable = rawVal == "true"
		// [render]
		case "render.fps":
			m.FPS, _ = strconv.Atoi(rawVal)
		case "render.vsync":
			m.VSync = rawVal == "true"
		case "render.clear_color":
			m.ClearColor = parseColorArray(rawVal)
		// [build]
		case "build.entry":
			m.Entry = unquote(rawVal)
		case "build.target":
			m.Target = unquote(rawVal)
		// Flat (no section) — for simple groot.mod files.
		case ".fps_limit":
			m.FPS, _ = strconv.Atoi(rawVal)
		case ".width":
			m.Width, _ = strconv.Atoi(rawVal)
		case ".height":
			m.Height, _ = strconv.Atoi(rawVal)
		case ".resizable":
			m.Resizable = rawVal == "true"
		case ".default_clear":
			m.ClearColor = parseColorArray(rawVal)
		}
	}
	return m, scanner.Err()
}

func unquote(s string) string {
	if len(s) >= 2 && s[0] == '"' && s[len(s)-1] == '"' {
		return s[1 : len(s)-1]
	}
	return s
}

// parseColorArray parses "[r, g, b, a]" or "[r, g, b]" into a [4]float64.
func parseColorArray(s string) [4]float64 {
	s = strings.TrimSpace(s)
	s = strings.TrimPrefix(s, "[")
	s = strings.TrimSuffix(s, "]")
	parts := strings.Split(s, ",")
	var arr [4]float64
	arr[3] = 1.0 // default alpha
	for i, p := range parts {
		if i >= 4 {
			break
		}
		arr[i], _ = strconv.ParseFloat(strings.TrimSpace(p), 64)
	}
	return arr
}

// EntryFile returns the absolute path to the project's main.ube (or configured
// entry point).
func (m *GrootManifest) EntryFile() string {
	return filepath.Join(m.Dir, filepath.FromSlash(m.Entry))
}

// DefaultWasmOutput returns the conventional output .wasm path for this project.
func (m *GrootManifest) DefaultWasmOutput() string {
	return filepath.Join(m.Dir, m.Name+".wasm")
}
