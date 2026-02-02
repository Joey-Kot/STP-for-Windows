package request

import (
	"encoding/json"
	"strings"
)

type RuntimeOverrides struct {
	APIEndpoint string
	Token       string
	TEXTPath    string
}

func ParseExtraConfig(raw string) (map[string]interface{}, error) {
	raw = strings.TrimSpace(raw)
	if raw == "" {
		return nil, nil
	}
	m := make(map[string]interface{})
	if err := json.Unmarshal([]byte(raw), &m); err != nil {
		return nil, err
	}
	return m, nil
}

func ExtractRuntimeOverrides(extra map[string]interface{}) (RuntimeOverrides, map[string]interface{}) {
	if extra == nil {
		return RuntimeOverrides{}, nil
	}
	clean := make(map[string]interface{}, len(extra))
	for k, v := range extra {
		clean[k] = v
	}
	out := RuntimeOverrides{}
	if v, ok := clean["APIEndpoint"]; ok {
		if s, ok := v.(string); ok {
			out.APIEndpoint = strings.TrimSpace(s)
		}
		delete(clean, "APIEndpoint")
	}
	if v, ok := clean["Token"]; ok {
		if s, ok := v.(string); ok {
			out.Token = strings.TrimSpace(s)
		}
		delete(clean, "Token")
	}
	if v, ok := clean["TEXTPath"]; ok {
		if s, ok := v.(string); ok {
			out.TEXTPath = strings.TrimSpace(s)
		}
		delete(clean, "TEXTPath")
	}
	return out, clean
}

func MergeExtra(base map[string]interface{}, override map[string]interface{}) map[string]interface{} {
	if base == nil && override == nil {
		return nil
	}
	out := make(map[string]interface{})
	for k, v := range base {
		out[k] = v
	}
	for k, v := range override {
		out[k] = v
	}
	if len(out) == 0 {
		return nil
	}
	return out
}

func StripEmptyFields(m map[string]interface{}) map[string]interface{} {
	if m == nil {
		return nil
	}
	out := make(map[string]interface{}, len(m))
	for k, v := range m {
		cleaned := cleanValue(v)
		if cleaned == nil {
			continue
		}
		out[k] = cleaned
	}
	return out
}

func cleanValue(v interface{}) interface{} {
	if v == nil {
		return nil
	}
	switch t := v.(type) {
	case string:
		if strings.TrimSpace(t) == "" {
			return nil
		}
		return t
	case map[string]interface{}:
		m := StripEmptyFields(t)
		if len(m) == 0 {
			return nil
		}
		return m
	case []interface{}:
		arr := make([]interface{}, 0, len(t))
		for _, item := range t {
			cleaned := cleanValue(item)
			if cleaned != nil {
				arr = append(arr, cleaned)
			}
		}
		if len(arr) == 0 {
			return nil
		}
		return arr
	default:
		return v
	}
}
