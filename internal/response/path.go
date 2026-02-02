package response

import (
	"fmt"
	"strconv"
	"strings"
)

func extractByPath(root interface{}, path string) (string, bool) {
	if path == "" {
		return "", false
	}
	parts := strings.Split(path, ".")
	cur := root
	for _, part := range parts {
		key, idxs, err := parseKeyAndIndexes(part)
		if err != nil {
			return "", false
		}
		if key != "" {
			m, ok := cur.(map[string]interface{})
			if !ok {
				return "", false
			}
			next, ok := m[key]
			if !ok {
				return "", false
			}
			cur = next
		}
		for _, idx := range idxs {
			arr, ok := cur.([]interface{})
			if !ok {
				return "", false
			}
			if idx < 0 || idx >= len(arr) {
				return "", false
			}
			cur = arr[idx]
		}
	}
	switch v := cur.(type) {
	case string:
		return v, true
	case float64:
		if v == float64(int64(v)) {
			return fmt.Sprintf("%d", int64(v)), true
		}
		return fmt.Sprintf("%v", v), true
	case bool:
		return fmt.Sprintf("%v", v), true
	default:
		return "", false
	}
}

func parseKeyAndIndexes(token string) (string, []int, error) {
	if token == "" {
		return "", nil, fmt.Errorf("empty token")
	}
	idxs := []int{}
	br := strings.Index(token, "[")
	if br < 0 {
		return token, idxs, nil
	}
	key := token[:br]
	rest := token[br:]
	for len(rest) > 0 {
		if !strings.HasPrefix(rest, "[") {
			return "", nil, fmt.Errorf("invalid index syntax: %s", token)
		}
		closePos := strings.Index(rest, "]")
		if closePos < 0 {
			return "", nil, fmt.Errorf("missing closing ]: %s", token)
		}
		n, err := strconv.Atoi(rest[1:closePos])
		if err != nil {
			return "", nil, err
		}
		idxs = append(idxs, n)
		rest = rest[closePos+1:]
	}
	return key, idxs, nil
}
