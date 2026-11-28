// File: cmd/groot/ui.go
// Shared terminal formatting helpers for the groot CLI.
package main

import (
	"fmt"
	"os"
	"time"
)

const (
	colorReset      = "\x1b[0m"
	colorGreen      = "\x1b[38;5;48m"
	colorPurple     = "\x1b[38;2;147;51;234m"
	colorPurpleBold = "\x1b[1;38;2;147;51;234m"
	colorDim        = "\x1b[2m"
)

func paint(text, color string) string {
	info, err := os.Stdout.Stat()
	if err != nil || info.Mode()&os.ModeCharDevice == 0 {
		return text
	}
	return color + text + colorReset
}

func startSpinner(msg string) func() {
	stop := make(chan struct{})
	frames := []string{"⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"}
	go func() {
		idx := 0
		for {
			select {
			case <-stop:
				fmt.Print("\r\033[K")
				return
			case <-time.After(60 * time.Millisecond):
				fmt.Printf("\r%s %s", paint(frames[idx%len(frames)], colorPurple), paint(msg, colorDim))
				idx++
			}
		}
	}()
	return func() {
		close(stop)
		fmt.Print("\r\033[K")
	}
}
