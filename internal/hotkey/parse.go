package hotkey

import (
	"fmt"
	"strconv"
	"strings"
)

const (
	VK_NUMPAD0  = 0x60
	VK_NUMPAD1  = 0x61
	VK_NUMPAD2  = 0x62
	VK_NUMPAD3  = 0x63
	VK_NUMPAD4  = 0x64
	VK_NUMPAD5  = 0x65
	VK_NUMPAD6  = 0x66
	VK_NUMPAD7  = 0x67
	VK_NUMPAD8  = 0x68
	VK_NUMPAD9  = 0x69
	VK_ADD      = 0x6B
	VK_SUBTRACT = 0x6D
)

func ParseHotkey(s string) (uint32, uint32, error) {
	if s == "" {
		return 0, 0, fmt.Errorf("empty key")
	}
	parts := strings.Split(s, "+")
	for i := range parts {
		parts[i] = strings.TrimSpace(strings.ToLower(parts[i]))
	}
	var mod uint32
	keyToken := parts[len(parts)-1]
	for _, p := range parts[:len(parts)-1] {
		switch p {
		case "alt", "menu":
			mod |= 0x0001
		case "ctrl", "control":
			mod |= 0x0002
		case "shift":
			mod |= 0x0004
		case "win", "meta", "super":
			mod |= 0x0008
		}
	}
	if len(keyToken) == 1 {
		ch := keyToken[0]
		if ch >= 'a' && ch <= 'z' {
			return mod, uint32(ch - 'a' + 'A'), nil
		}
		if ch >= '0' && ch <= '9' {
			return mod, uint32(ch), nil
		}
	}
	switch keyToken {
	case "esc", "escape":
		return mod, 0x1B, nil
	case "space":
		return mod, 0x20, nil
	case "enter", "return":
		return mod, 0x0D, nil
	case "tab":
		return mod, 0x09, nil
	case "backspace":
		return mod, 0x08, nil
	case "insert":
		return mod, 0x2D, nil
	case "delete":
		return mod, 0x2E, nil
	case "home":
		return mod, 0x24, nil
	case "end":
		return mod, 0x23, nil
	case "pageup":
		return mod, 0x21, nil
	case "pagedown":
		return mod, 0x22, nil
	case "left":
		return mod, 0x25, nil
	case "up":
		return mod, 0x26, nil
	case "right":
		return mod, 0x27, nil
	case "down":
		return mod, 0x28, nil
	}
	if strings.HasPrefix(keyToken, "f") {
		if n, err := strconv.Atoi(strings.TrimPrefix(keyToken, "f")); err == nil && n >= 1 && n <= 24 {
			return mod, 0x70 + uint32(n-1), nil
		}
	}
	switch keyToken {
	case "numpad0", "num0", "kp0":
		return mod, VK_NUMPAD0, nil
	case "numpad1", "num1", "kp1":
		return mod, VK_NUMPAD1, nil
	case "numpad2", "num2", "kp2":
		return mod, VK_NUMPAD2, nil
	case "numpad3", "num3", "kp3":
		return mod, VK_NUMPAD3, nil
	case "numpad4", "num4", "kp4":
		return mod, VK_NUMPAD4, nil
	case "numpad5", "num5", "kp5":
		return mod, VK_NUMPAD5, nil
	case "numpad6", "num6", "kp6":
		return mod, VK_NUMPAD6, nil
	case "numpad7", "num7", "kp7":
		return mod, VK_NUMPAD7, nil
	case "numpad8", "num8", "kp8":
		return mod, VK_NUMPAD8, nil
	case "numpad9", "num9", "kp9":
		return mod, VK_NUMPAD9, nil
	case "add", "plus", "kpadd":
		return mod, VK_ADD, nil
	case "subtract", "minus", "kpsubtract":
		return mod, VK_SUBTRACT, nil
	}
	return 0, 0, fmt.Errorf("unsupported key token: %s", s)
}
