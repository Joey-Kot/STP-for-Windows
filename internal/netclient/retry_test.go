// Copyright (C) 2026 Joey Kot <joey.kot.x@gmail.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed WITHOUT ANY WARRANTY; without even the
// implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
// See <https://www.gnu.org/licenses/> for more details.

package netclient

import (
	"context"
	"fmt"
	"io"
	"net/http"
	"strings"
	"testing"
	"time"
)

type fakeDoer struct {
	attempts int
	fn       func(*http.Request, int) (*http.Response, error)
}

func (f *fakeDoer) Do(req *http.Request) (*http.Response, error) {
	f.attempts++
	return f.fn(req, f.attempts)
}

func TestSendWithRetrySuccessAfterFailures(t *testing.T) {
	d := &fakeDoer{fn: func(req *http.Request, attempt int) (*http.Response, error) {
		if attempt < 3 {
			return nil, fmt.Errorf("transient")
		}
		return &http.Response{
			StatusCode: 200,
			Body:       io.NopCloser(strings.NewReader(`{"ok":true}`)),
		}, nil
	}}
	var sleepCalls int
	body, err := SendWithRetry(context.Background(), d, "https://example", "", map[string]interface{}{"x": 1}, RetryOptions{
		MaxRetry:  3,
		BaseDelay: time.Millisecond,
		Sleep: func(ctx context.Context, d time.Duration) error {
			sleepCalls++
			return nil
		},
	})
	if err != nil {
		t.Fatal(err)
	}
	if string(body) != `{"ok":true}` {
		t.Fatalf("unexpected body: %s", string(body))
	}
	if d.attempts != 3 || sleepCalls != 2 {
		t.Fatalf("unexpected retries attempts=%d sleepCalls=%d", d.attempts, sleepCalls)
	}
}

func TestSendWithRetryCanceledDuringBackoff(t *testing.T) {
	d := &fakeDoer{fn: func(req *http.Request, attempt int) (*http.Response, error) {
		return nil, fmt.Errorf("always fail")
	}}
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	_, err := SendWithRetry(ctx, d, "https://example", "", map[string]interface{}{}, RetryOptions{
		MaxRetry:  3,
		BaseDelay: time.Second,
		Sleep: func(ctx context.Context, d time.Duration) error {
			cancel()
			<-ctx.Done()
			return ctx.Err()
		},
	})
	if err == nil {
		t.Fatalf("expected cancel error")
	}
	if ctx.Err() == nil {
		t.Fatalf("ctx should be canceled")
	}
}
