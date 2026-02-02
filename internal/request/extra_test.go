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
