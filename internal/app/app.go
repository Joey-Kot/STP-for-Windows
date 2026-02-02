package app

import (
	"context"
	"fmt"
	"strings"
	"sync"
	"time"

	"stp/internal/clipboard"
	"stp/internal/config"
	"stp/internal/netclient"
	"stp/internal/request"
	"stp/internal/response"
)

type App struct {
	cfg         config.Config
	httpDoer    netclient.Doer
	textIO      clipboard.TextIO
	globalExtra map[string]interface{}

	eventCh chan int
	stopCh  chan struct{}

	mu            sync.Mutex
	currentCancel context.CancelFunc
	closed        bool
	wg            sync.WaitGroup
}

func New(cfg config.Config, httpDoer netclient.Doer, textIO clipboard.TextIO) (*App, error) {
	globalExtra, err := request.ParseExtraConfig(cfg.ExtraConfig)
	if err != nil {
		return nil, fmt.Errorf("invalid ExtraConfig JSON: %w", err)
	}
	return &App{
		cfg:         cfg,
		httpDoer:    httpDoer,
		textIO:      textIO,
		globalExtra: globalExtra,
		eventCh:     make(chan int, 64),
		stopCh:      make(chan struct{}),
	}, nil
}

func (a *App) Start() {
	a.wg.Add(1)
	go func() {
		defer a.wg.Done()
		for {
			select {
			case <-a.stopCh:
				return
			case id := <-a.eventCh:
				a.handleTask(id)
			}
		}
	}()
}

func (a *App) Close() {
	a.mu.Lock()
	if a.closed {
		a.mu.Unlock()
		return
	}
	a.closed = true
	close(a.stopCh)
	if a.currentCancel != nil {
		a.currentCancel()
		a.currentCancel = nil
	}
	a.mu.Unlock()
	a.wg.Wait()
}

func (a *App) EnqueueTask(id int) {
	a.mu.Lock()
	closed := a.closed
	a.mu.Unlock()
	if closed {
		return
	}
	select {
	case a.eventCh <- id:
	default:
		if a.cfg.DEBUG {
			fmt.Printf("[app] queue full, dropped task id=%d\n", id)
		}
	}
}

func (a *App) StopAll() {
	a.mu.Lock()
	if a.currentCancel != nil {
		a.currentCancel()
	}
	a.mu.Unlock()

	for {
		select {
		case <-a.eventCh:
		default:
			return
		}
	}
}

func (a *App) setCurrentCancel(cancel context.CancelFunc) {
	a.mu.Lock()
	defer a.mu.Unlock()
	a.currentCancel = cancel
}

func (a *App) clearCurrentCancel(cancel context.CancelFunc) {
	a.mu.Lock()
	defer a.mu.Unlock()
	a.currentCancel = nil
}

func (a *App) handleTask(id int) {
	if id < 1 || id > len(a.cfg.HotKeyConfig) {
		return
	}
	entry := a.cfg.HotKeyConfig[id-1]
	prompt := strings.TrimSpace(entry.Prompt)
	if prompt == "" {
		return
	}

	selectedText, err := a.textIO.CopySelected()
	if err != nil || strings.TrimSpace(selectedText) == "" {
		if a.cfg.DEBUG && err != nil {
			fmt.Printf("[copy] failed: %v\n", err)
		}
		return
	}

	perExtra, err := request.ParseExtraConfig(entry.ExtraConfig)
	if err != nil {
		if a.cfg.DEBUG {
			fmt.Printf("[request] invalid entry ExtraConfig id=%d: %v\n", id, err)
		}
		perExtra = nil
	}
	runtimeOverrides, perExtraClean := request.ExtractRuntimeOverrides(perExtra)

	payload := request.BuildPayload(request.BuildInput{
		Model:       a.cfg.Model,
		Temperature: a.cfg.Temperature,
		MaxTokens:   a.cfg.MaxTokens,
		Prompt:      prompt,
		UserText:    selectedText,
		Extra:       request.MergeExtra(a.globalExtra, perExtraClean),
	})

	ctx, cancel := context.WithCancel(context.Background())
	a.setCurrentCancel(cancel)
	defer func() {
		cancel()
		a.clearCurrentCancel(cancel)
	}()

	endpoint := strings.TrimSpace(runtimeOverrides.APIEndpoint)
	if endpoint == "" {
		endpoint = strings.TrimSpace(a.cfg.APIEndpoint)
	}
	token := strings.TrimSpace(runtimeOverrides.Token)
	if token == "" {
		token = strings.TrimSpace(a.cfg.Token)
	}

	resBody, err := netclient.SendWithRetry(ctx, a.httpDoer, endpoint, token, payload, netclient.RetryOptions{
		MaxRetry:  a.cfg.MaxRetry,
		BaseDelay: time.Duration(a.cfg.RetryBaseDelay * float64(time.Second)),
		Debug:     a.cfg.DEBUG,
	})
	if err != nil {
		if a.cfg.DEBUG {
			fmt.Printf("[request] failed: %v\n", err)
		}
		a.notifyPlaceholder("[request failed]")
		return
	}

	extracted := response.ExtractTextFromResponse(resBody, runtimeOverrides.TEXTPath, a.cfg.TEXTPath)
	if strings.TrimSpace(extracted) == "" {
		a.notifyPlaceholder("[empty result]")
		return
	}
	if err := a.textIO.PasteText(extracted); err != nil && a.cfg.DEBUG {
		fmt.Printf("[paste] failed: %v\n", err)
	}
}

func (a *App) notifyPlaceholder(text string) {
	if !a.cfg.RequestFailedNotification {
		return
	}
	_ = a.textIO.PasteText(text)
}
