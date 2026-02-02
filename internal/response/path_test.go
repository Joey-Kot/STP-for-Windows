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
