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

package response

import "testing"

func TestExtractByPath(t *testing.T) {
	json := []byte(`{"choices":[{"message":{"content":"hello"}}],"num":12}`)
	got := ExtractTextFromResponse(json, "choices[0].message.content", "")
	if got != "hello" {
		t.Fatalf("expected hello, got %q", got)
	}
	gotNum := ExtractTextFromResponse(json, "num", "")
	if gotNum != "12" {
		t.Fatalf("expected 12, got %q", gotNum)
	}
}

func TestExtractFallback(t *testing.T) {
	json := []byte(`{"text":"fallback"}`)
	got := ExtractTextFromResponse(json, "bad.path", "")
	if got != "fallback" {
		t.Fatalf("expected fallback, got %q", got)
	}
}
