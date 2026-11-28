// File: cmd/groot/monitor.go
// Implements `groot monitor` — connects to a running Groot game's IPC socket
// and displays the live ratatui TUI dashboard.
package main

import (
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
)

// GrootSession is the session discovery record written by the host runner to
// /tmp/groot-active.json when it starts in debug mode.
type GrootSession struct {
	PID         int    `json:"pid"`
	ProjectName string `json:"project_name"`
	Title       string `json:"title"`
	Mode        string `json:"mode"`
	SocketPath  string `json:"socket_path"`
	StartedAt   int64  `json:"started_at"`
}

func runMonitor(args []string) error {
	pid := 0
	for _, arg := range args {
		if n, err := strconv.Atoi(arg); err == nil {
			pid = n
		}
	}

	session, err := discoverSession(pid)
	if err != nil {
		return fmt.Errorf("no running Groot game found.\n\nStart one with: groot start . --monitor\n\nDetails: %w", err)
	}

	if session.Mode == "release" {
		return fmt.Errorf("game %q is running in release mode — telemetry is disabled.\nRestart with 'groot start . --debug' to enable the monitor", session.ProjectName)
	}

	fmt.Printf("%s %s %s\n",
		paint("◆", colorPurpleBold),
		paint("groot monitor", colorPurple),
		paint(fmt.Sprintf("connecting to '%s' (pid %d)", session.ProjectName, session.PID), colorDim),
	)

	return launchMonitorTUIWithSession(session)
}

// discoverSession finds an active Groot session. If pid > 0 it looks for
// /tmp/groot-<pid>.sock, otherwise it reads /tmp/groot-active.json.
func discoverSession(pid int) (*GrootSession, error) {
	var sessionFile string
	if pid > 0 {
		sessionFile = fmt.Sprintf("/tmp/groot-%d.json", pid)
	} else {
		sessionFile = "/tmp/groot-active.json"
	}

	data, err := os.ReadFile(sessionFile)
	if err != nil {
		return nil, fmt.Errorf("reading session file %s: %w", sessionFile, err)
	}

	var session GrootSession
	if err := json.Unmarshal(data, &session); err != nil {
		return nil, fmt.Errorf("parsing session file: %w", err)
	}

	// Verify the socket exists.
	if _, err := os.Stat(session.SocketPath); err != nil {
		return nil, fmt.Errorf("IPC socket %s not found — is the game still running?", session.SocketPath)
	}

	return &session, nil
}

// launchMonitorWindow launches groot-monitor in a new terminal window if available,
// or spawns `groot monitor` in a separate process so it doesn't fight the game process for stdin/stdout.
func launchMonitorWindow() error {
	session, err := discoverSession(0)
	if err != nil {
		return err
	}

	monitorBin, err := findMonitorBin()
	if err != nil {
		return runBuiltinMonitor(session)
	}

	args := []string{
		"--socket=" + session.SocketPath,
		"--project=" + session.ProjectName,
		"--pid=" + strconv.Itoa(session.PID),
	}

	// Try spawning in a new terminal window
	terminals := []struct {
		bin  string
		args []string
	}{
		{"x-terminal-emulator", append([]string{"-e", monitorBin}, args...)},
		{"gnome-terminal", append([]string{"--", monitorBin}, args...)},
		{"alacritty", append([]string{"-e", monitorBin}, args...)},
		{"kitty", append([]string{monitorBin}, args...)},
		{"xterm", append([]string{"-e", monitorBin}, args...)},
	}

	for _, t := range terminals {
		if path, err := exec.LookPath(t.bin); err == nil {
			cmd := exec.Command(path, t.args...)
			if err := cmd.Start(); err == nil {
				return nil
			}
		}
	}

	fmt.Printf("\n%s %s: run %s in another terminal tab/window\n",
		paint("◆", colorPurpleBold),
		paint("Groot Monitor", colorPurple),
		paint("groot monitor", colorDim),
	)
	return nil
}

// launchMonitorTUI starts the monitor for a discovered active session.
func launchMonitorTUI() error {
	session, err := discoverSession(0)
	if err != nil {
		return err
	}
	return launchMonitorTUIWithSession(session)
}

// launchMonitorTUIWithSession launches the groot-monitor binary (a ratatui TUI)
// connected to the session's IPC socket.
func launchMonitorTUIWithSession(session *GrootSession) error {
	monitorBin, err := findMonitorBin()
	if err != nil {
		// Fall back to the built-in ASCII monitor when the ratatui binary is
		// not compiled yet.
		return runBuiltinMonitor(session)
	}

	cmd := exec.Command(monitorBin,
		"--socket="+session.SocketPath,
		"--project="+session.ProjectName,
		"--pid="+strconv.Itoa(session.PID),
	)
	cmd.Stdin = os.Stdin
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr
	return cmd.Run()
}

// findMonitorBin locates the groot-monitor binary (the ratatui TUI).
func findMonitorBin() (string, error) {
	if v := os.Getenv("GROOT_MONITOR"); v != "" {
		return v, nil
	}
	if p, err := exec.LookPath("groot-monitor"); err == nil {
		return p, nil
	}
	// Look inside the _groot target directories relative to CWD.
	wd, _ := os.Getwd()
	curr := wd
	for {
		for _, profile := range []string{"debug", "release"} {
			candidate := filepath.Join(curr, "_groot", "target", profile, "groot-monitor")
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
	return "", fmt.Errorf("groot-monitor binary not found")
}

// runBuiltinMonitor is a lightweight fallback monitor that prints live stats
// to the terminal when the ratatui binary is not available.
func runBuiltinMonitor(session *GrootSession) error {
	fmt.Printf("\n%s\n", strings.Repeat("─", 70))
	fmt.Printf("  Groot Engine Monitor — %s (pid %d) — Mode: %s\n",
		session.ProjectName, session.PID, strings.ToUpper(session.Mode))
	fmt.Printf("  IPC socket: %s\n", session.SocketPath)
	fmt.Printf("%s\n\n", strings.Repeat("─", 70))

	reader, err := connectToSocket(session.SocketPath)
	if err != nil {
		return fmt.Errorf("connecting to IPC socket: %w", err)
	}
	defer reader.Close()

	fmt.Println("Connected. Receiving telemetry packets… (Ctrl+C to quit)")
	fmt.Printf("\n%-10s %-8s %-10s %-12s %-10s %-10s\n",
		"Frame", "FPS", "DeltaUs", "ArenaPeak", "Entities", "DrawCalls")
	fmt.Println(strings.Repeat("-", 65))

	return reader.ReadLoop(func(p *TelemetryPacket) {
		fmt.Printf("%-10d %-8.1f %-10d %-12d %-10d %-10d\n",
			p.FrameIndex,
			p.FPS,
			p.FrameTimeUs,
			p.ArenaPeakBytes,
			p.EntitiesAlive,
			p.DrawCalls,
		)
	})
}
