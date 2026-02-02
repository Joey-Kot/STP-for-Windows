package netclient

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"time"
)

type RetryOptions struct {
	MaxRetry  int
	BaseDelay time.Duration
	Debug     bool
	Sleep     func(context.Context, time.Duration) error
	UserAgent string
}

func SendWithRetry(ctx context.Context, doer Doer, endpoint, token string, payload map[string]interface{}, opts RetryOptions) ([]byte, error) {
	if endpoint == "" {
		return nil, fmt.Errorf("API endpoint empty")
	}
	if opts.MaxRetry <= 0 {
		opts.MaxRetry = 1
	}
	if opts.BaseDelay < 0 {
		opts.BaseDelay = 0
	}
	if opts.Sleep == nil {
		opts.Sleep = sleepWithContext
	}
	if opts.UserAgent == "" {
		opts.UserAgent = "clip-hotkey-client/1.0"
	}

	data, err := json.Marshal(payload)
	if err != nil {
		return nil, err
	}

	delay := opts.BaseDelay
	var lastErr error
	for attempt := 1; attempt <= opts.MaxRetry; attempt++ {
		req, err := http.NewRequestWithContext(ctx, http.MethodPost, endpoint, bytes.NewBuffer(data))
		if err != nil {
			return nil, err
		}
		req.Header.Set("Content-Type", "application/json")
		req.Header.Set("User-Agent", opts.UserAgent)
		if token != "" {
			req.Header.Set("Authorization", "Bearer "+token)
		}

		resp, err := doer.Do(req)
		if err != nil {
			if ctx.Err() != nil {
				return nil, ctx.Err()
			}
			lastErr = err
		} else {
			body, readErr := io.ReadAll(resp.Body)
			_ = resp.Body.Close()
			if readErr != nil {
				lastErr = readErr
			} else if resp.StatusCode >= 200 && resp.StatusCode < 300 {
				return body, nil
			} else {
				lastErr = fmt.Errorf("status %d: %s", resp.StatusCode, string(body))
			}
		}

		if attempt == opts.MaxRetry {
			break
		}
		if err := opts.Sleep(ctx, delay); err != nil {
			return nil, err
		}
		delay *= 2
	}
	if lastErr == nil {
		lastErr = fmt.Errorf("request failed")
	}
	return nil, lastErr
}

func sleepWithContext(ctx context.Context, d time.Duration) error {
	if d <= 0 {
		select {
		case <-ctx.Done():
			return ctx.Err()
		default:
			return nil
		}
	}
	t := time.NewTimer(d)
	defer t.Stop()
	select {
	case <-ctx.Done():
		return ctx.Err()
	case <-t.C:
		return nil
	}
}
