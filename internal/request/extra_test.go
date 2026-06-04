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

package request

import "testing"

func TestMergeExtraAndNullDeletion(t *testing.T) {
	global := map[string]interface{}{
		"model":      "x",
		"max_tokens": 100,
		"verbosity":  "low",
	}
	entry := map[string]interface{}{
		"max_tokens": nil,
		"verbosity":  "",
		"new_field":  true,
	}
	merged := MergeExtra(global, entry)
	clean := StripEmptyFields(merged)
	if _, ok := clean["max_tokens"]; ok {
		t.Fatalf("nil max_tokens should be removed")
	}
	if _, ok := clean["verbosity"]; ok {
		t.Fatalf("empty verbosity should be removed")
	}
	if clean["new_field"] != true {
		t.Fatalf("new_field should remain")
	}
}

func TestExtractRuntimeOverrides(t *testing.T) {
	extra := map[string]interface{}{
		"APIEndpoint": "https://x",
		"Token":       "abc",
		"TEXTPath":    "choices[0].message.content",
		"temperature": 0,
	}
	o, clean := ExtractRuntimeOverrides(extra)
	if o.APIEndpoint != "https://x" || o.Token != "abc" || o.TEXTPath == "" {
		t.Fatalf("overrides not extracted correctly: %#v", o)
	}
	if _, ok := clean["APIEndpoint"]; ok {
		t.Fatalf("APIEndpoint should be removed from clean map")
	}
	if clean["temperature"] != 0 {
		t.Fatalf("non-special field should be retained")
	}
}
