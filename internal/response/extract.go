package response

import (
	"encoding/json"
	"strings"
)

func ExtractTextFromResponse(body []byte, overrideTEXTPath, defaultTEXTPath string) string {
	var root interface{}
	if err := json.Unmarshal(body, &root); err != nil {
		return ""
	}
	textPath := strings.TrimSpace(overrideTEXTPath)
	if textPath == "" {
		textPath = strings.TrimSpace(defaultTEXTPath)
	}
	if textPath != "" {
		if out, ok := extractByPath(root, textPath); ok {
			return out
		}
	}
	m, ok := root.(map[string]interface{})
	if !ok {
		return ""
	}
	if v, exists := m["text"]; exists {
		if s, ok := v.(string); ok {
			return s
		}
	}
	for _, v := range m {
		if s, ok := v.(string); ok && s != "" {
			return s
		}
	}
	return ""
}
