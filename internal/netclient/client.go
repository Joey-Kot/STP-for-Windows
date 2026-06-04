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
