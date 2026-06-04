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
