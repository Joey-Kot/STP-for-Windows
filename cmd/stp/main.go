package main

import (
	"fmt"
	"os"
	"os/signal"
	"path/filepath"
	"strings"
	"syscall"
	"time"

	"stp/internal/app"
	"stp/internal/clipboard"
	"stp/internal/config"
	"stp/internal/hotkey"
	"stp/internal/keyboard"
	"stp/internal/netclient"
)

func main() {
	program := filepath.Base(os.Args[0])
	opts, err := config.ParseCLI(os.Args[1:], os.Stderr)
	if err != nil {
		os.Exit(2)
	}
	if opts.ShowHelp {
		config.Usage(os.Stderr, program)
		return
	}

	cfg, err := loadConfigWithFallback(opts)
	if err != nil {
		fmt.Printf("[main] %v\n", err)
		os.Exit(1)
	}
	config.ApplyCLI(&cfg, opts)

	httpClient, transport := netclient.New(cfg)
	defer transport.CloseIdleConnections()

	textIO := &clipboard.Manager{
		Clipboard: &clipboard.SystemClipboard{},
		Keyboard:  keyboard.NewSystemKeySimulator(),
		Timeout:   time.Duration(cfg.ClipboardTimeout) * time.Millisecond,
		Debug:     cfg.DEBUG,
	}

	application, err := app.New(cfg, httpClient, textIO)
	if err != nil {
		fmt.Printf("[main] %v\n", err)
		os.Exit(1)
	}
	application.Start()
	defer application.Close()

	taskSpecs := map[int]string{}
	for i, entry := range cfg.HotKeyConfig {
		if strings.TrimSpace(entry.Prompt) != "" && strings.TrimSpace(entry.HotKey) != "" {
			taskSpecs[i+1] = entry.HotKey
		}
	}
	if len(taskSpecs) == 0 {
		fmt.Println("[main] no prompts configured; nothing to register. Exiting.")
		return
	}

	hotkeyService := hotkey.NewService(hotkey.Options{
		UseHook:        cfg.HotKeyHook,
		TaskHotkeys:    taskSpecs,
		StopTaskHotkey: cfg.StopTaskHotkey,
		Debug:          cfg.DEBUG,
	})
	if err := hotkeyService.Start(func(ev hotkey.Event) {
		switch ev.Type {
		case hotkey.StopEvent:
			application.StopAll()
		case hotkey.TaskEvent:
			application.EnqueueTask(ev.TaskID)
		}
	}); err != nil {
		fmt.Printf("[main] failed to start hotkey service: %v\n", err)
		os.Exit(1)
	}
	defer hotkeyService.Close()

	sigCh := make(chan os.Signal, 1)
	signal.Notify(sigCh, os.Interrupt, syscall.SIGTERM)
	fmt.Println("[main] ready. Press configured hotkeys to invoke. Ctrl+C to exit.")
	<-sigCh
	fmt.Println("[main] exiting")
}

func loadConfigWithFallback(opts config.CLIOptions) (config.Config, error) {
	if opts.ConfigPath != "" {
		return config.Load(opts.ConfigPath)
	}
	if _, err := os.Stat("config.json"); err == nil {
		return config.Load("config.json")
	} else if os.IsNotExist(err) {
		if !opts.AnyOverrideSet() {
			if err := config.SaveDefault("config.json"); err != nil {
				return config.Config{}, fmt.Errorf("failed create default config: %w", err)
			}
			fmt.Println("[main] default config.json created. Please edit it and re-run.")
			os.Exit(0)
		}
		return config.Default(), nil
	} else {
		return config.Config{}, fmt.Errorf("stat config.json failed: %w", err)
	}
}
