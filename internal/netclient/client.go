package netclient

import (
	"crypto/tls"
	"net/http"
	"time"

	"golang.org/x/net/http2"

	"stp/internal/config"
)

type Doer interface {
	Do(req *http.Request) (*http.Response, error)
}

func New(cfg config.Config) (*http.Client, *http.Transport) {
	tr := &http.Transport{
		MaxIdleConns:        100,
		MaxIdleConnsPerHost: 100,
		IdleConnTimeout:     90 * time.Second,
	}
	if !cfg.VerifySSL {
		tr.TLSClientConfig = &tls.Config{InsecureSkipVerify: true}
	}
	if cfg.EnableHTTP2 {
		_ = http2.ConfigureTransport(tr)
	}
	cli := &http.Client{
		Transport: tr,
		Timeout:   time.Duration(cfg.RequestTimeout) * time.Second,
	}
	return cli, tr
}
