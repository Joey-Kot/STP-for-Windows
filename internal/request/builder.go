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

type BuildInput struct {
	Model       string
	Temperature float64
	MaxTokens   int
	Prompt      string
	UserText    string
	Extra       map[string]interface{}
}

func BuildPayload(in BuildInput) map[string]interface{} {
	payload := make(map[string]interface{})
	if in.Model != "" {
		payload["model"] = in.Model
	}
	payload["messages"] = []map[string]string{
		{"role": "developer", "content": in.Prompt},
		{"role": "user", "content": in.UserText},
	}
	if in.MaxTokens > 0 {
		payload["max_tokens"] = in.MaxTokens
	}
	payload["temperature"] = in.Temperature
	for k, v := range in.Extra {
		payload[k] = v
	}
	return StripEmptyFields(payload)
}
