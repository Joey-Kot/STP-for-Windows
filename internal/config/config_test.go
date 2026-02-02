package config

import (
	"bytes"
	"os"
	"path/filepath"
	"testing"
)

func TestDefaultHasNewFields(t *testing.T) {
	cfg := Default()
	if cfg.RequestFailedNotification {
		t.Fatalf("RequestFailedNotification default should be false")
	}
	if cfg.StopTaskHotkey != "" {
		t.Fatalf("StopTaskHotkey default should be empty")
	}
}

func TestLoadAndCLIOverride(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "config.json")
	content := `{"APIEndpoint":"https://a","RequestFailedNotification":false,"StopTaskHotkey":"ctrl+f12"}`
	if err := os.WriteFile(path, []byte(content), 0o644); err != nil {
		t.Fatal(err)
	}
	cfg, err := Load(path)
	if err != nil {
		t.Fatal(err)
	}
	if cfg.StopTaskHotkey != "ctrl+f12" {
		t.Fatalf("unexpected loaded stop hotkey: %s", cfg.StopTaskHotkey)
	}

	var stderr bytes.Buffer
	opts, err := ParseCLI([]string{"-request-failed-notification=true", "-stop-task-hotkey=alt+q"}, &stderr)
	if err != nil {
		t.Fatal(err)
	}
	ApplyCLI(&cfg, opts)
	if !cfg.RequestFailedNotification {
		t.Fatalf("cli should override RequestFailedNotification=true")
	}
	if cfg.StopTaskHotkey != "alt+q" {
		t.Fatalf("cli should override StopTaskHotkey")
	}
}
