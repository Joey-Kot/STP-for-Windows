package app

import (
	"fmt"
	"io"
	"net/http"
	"strings"
	"sync"
	"testing"
	"time"

	"stp/internal/config"
)

type fakeTextIO struct {
	copyText string

	mu        sync.Mutex
	copyCalls int
	pasted    []string
}

func (f *fakeTextIO) CopySelected() (string, error) {
	f.mu.Lock()
	defer f.mu.Unlock()
	f.copyCalls++
	return f.copyText, nil
}

func (f *fakeTextIO) PasteText(text string) error {
	f.mu.Lock()
	defer f.mu.Unlock()
	f.pasted = append(f.pasted, text)
	return nil
}

func (f *fakeTextIO) pastedContains(s string) bool {
	f.mu.Lock()
	defer f.mu.Unlock()
	for _, p := range f.pasted {
		if p == s {
			return true
		}
	}
	return false
}

func (f *fakeTextIO) getCopyCalls() int {
	f.mu.Lock()
	defer f.mu.Unlock()
	return f.copyCalls
}

type fakeDoer struct {
	fn func(*http.Request) (*http.Response, error)
}

func (d fakeDoer) Do(req *http.Request) (*http.Response, error) {
	return d.fn(req)
}

func baseConfig() config.Config {
	cfg := config.Default()
	cfg.APIEndpoint = "https://example"
	cfg.MaxRetry = 1
	cfg.HotKeyConfig = []config.HotKeyEntry{{Prompt: "translate", HotKey: "ctrl+f1"}}
	return cfg
}

func TestRequestFailedNotificationOnRequestError(t *testing.T) {
	cfg := baseConfig()
	cfg.RequestFailedNotification = true
	ioMock := &fakeTextIO{copyText: "hello"}
	doer := fakeDoer{fn: func(req *http.Request) (*http.Response, error) {
		return nil, fmt.Errorf("network down")
	}}
	a, err := New(cfg, doer, ioMock)
	if err != nil {
		t.Fatal(err)
	}
	a.Start()
	defer a.Close()

	a.EnqueueTask(1)
	waitFor(t, func() bool { return ioMock.pastedContains("[request failed]") })
}

func TestRequestFailedNotificationOnEmptyResult(t *testing.T) {
	cfg := baseConfig()
	cfg.RequestFailedNotification = true
	ioMock := &fakeTextIO{copyText: "hello"}
	doer := fakeDoer{fn: func(req *http.Request) (*http.Response, error) {
		return &http.Response{StatusCode: 200, Body: io.NopCloser(strings.NewReader("{}"))}, nil
	}}
	a, err := New(cfg, doer, ioMock)
	if err != nil {
		t.Fatal(err)
	}
	a.Start()
	defer a.Close()

	a.EnqueueTask(1)
	waitFor(t, func() bool { return ioMock.pastedContains("[empty result]") })
}

func TestStopAllCancelsCurrentAndClearsQueue(t *testing.T) {
	cfg := baseConfig()
	cfg.RequestFailedNotification = false
	ioMock := &fakeTextIO{copyText: "hello"}

	started := make(chan struct{}, 1)
	doer := fakeDoer{fn: func(req *http.Request) (*http.Response, error) {
		select {
		case started <- struct{}{}:
		default:
		}
		<-req.Context().Done()
		return nil, req.Context().Err()
	}}
	a, err := New(cfg, doer, ioMock)
	if err != nil {
		t.Fatal(err)
	}
	a.Start()
	defer a.Close()

	a.EnqueueTask(1)
	<-started
	a.EnqueueTask(1)
	a.StopAll()

	time.Sleep(200 * time.Millisecond)
	if calls := ioMock.getCopyCalls(); calls != 1 {
		t.Fatalf("expected only first task to start, got copy calls=%d", calls)
	}
}

func TestConcurrentEnqueueProcessesSerially(t *testing.T) {
	cfg := baseConfig()
	cfg.RequestFailedNotification = false
	ioMock := &fakeTextIO{copyText: "hello"}
	doer := fakeDoer{fn: func(req *http.Request) (*http.Response, error) {
		return &http.Response{StatusCode: 200, Body: io.NopCloser(strings.NewReader(`{"text":"ok"}`))}, nil
	}}
	a, err := New(cfg, doer, ioMock)
	if err != nil {
		t.Fatal(err)
	}
	a.Start()
	defer a.Close()

	var wg sync.WaitGroup
	for i := 0; i < 20; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			a.EnqueueTask(1)
		}()
	}
	wg.Wait()
	waitFor(t, func() bool { return ioMock.getCopyCalls() == 20 })
}

func waitFor(t *testing.T, check func() bool) {
	t.Helper()
	deadline := time.Now().Add(2 * time.Second)
	for time.Now().Before(deadline) {
		if check() {
			return
		}
		time.Sleep(20 * time.Millisecond)
	}
	t.Fatalf("timeout waiting for condition")
}
