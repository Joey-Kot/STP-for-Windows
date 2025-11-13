package main

import (
	"bytes"
	"encoding/json"
	"flag"
	"fmt"
	"github.com/atotto/clipboard"
	"github.com/micmonay/keybd_event"
	"golang.org/x/net/http2"
	"io"
	"net/http"
	"os"
	"os/signal"
	"path/filepath"
	"runtime"
	"strconv"
	"strings"
	"sync"
	"syscall"
	"time"
	"crypto/tls"
	"errors"
	"unsafe"
)

type HotKeyEntry struct {
	Prompt      string `json:"Prompt"`
	HotKey      string `json:"HotKey"`
	ExtraConfig string `json:"ExtraConfig"`
}

type Config struct {
	APIEndpoint      string        `json:"APIEndpoint"`
	Token            string        `json:"Token"`
	Model            string        `json:"Model"`
	Temperature      float64       `json:"Temperature"`
	MaxTokens        int           `json:"Max_Tokens"`
	TEXTPath         string        `json:"TEXTPath"`
	ExtraConfig      string        `json:"ExtraConfig"`
	RequestTimeout   int           `json:"RequestTimeout"`
	MaxRetry         int           `json:"MaxRetry"`
	RetryBaseDelay   float64       `json:"RetryBaseDelay"`
	EnableHTTP2      bool          `json:"EnableHTTP2"`
	VerifySSL        bool          `json:"VerifySSL"`
	ClipboardTimeout int           `json:"ClipboardTimeout"`
	HotKeyConfig     []HotKeyEntry `json:"HotKeyConfig"`
	HotKeyHook       bool          `json:"HotKeyHook"`
	DEBUG            bool          `json:"DEBUG"`
}

var (
	cfg            Config
	flagConfigPath string
	flagOverrides  = map[string]*string{}
	extraConfigMap map[string]interface{}
	httpClient     *http.Client
	httpTransport  *http.Transport
	wg sync.WaitGroup
)

// defaultConfig builds default config with most fields empty, Temperature default 0.0, TEXTPath default per spec.
func defaultConfig() Config {
	return Config{
		APIEndpoint:    "",
		Token:          "",
		Model:          "",
		Temperature:    0.0,
		MaxTokens:      0,
		TEXTPath:       "choices[0].message.content",
		ExtraConfig:    "",
		RequestTimeout: 30,
		MaxRetry:       3,
		RetryBaseDelay: 0.5,
		EnableHTTP2:    true,
		VerifySSL:      true,
		ClipboardTimeout: 1000,
		HotKeyConfig: []HotKeyEntry{
			{Prompt: "", HotKey: "ctrl+f1", ExtraConfig: ""},
			{Prompt: "", HotKey: "ctrl+f2", ExtraConfig: ""},
			{Prompt: "", HotKey: "ctrl+f3", ExtraConfig: ""},
			{Prompt: "", HotKey: "ctrl+f4", ExtraConfig: ""},
			{Prompt: "", HotKey: "ctrl+f5", ExtraConfig: ""},
			{Prompt: "", HotKey: "ctrl+f6", ExtraConfig: ""},
			{Prompt: "", HotKey: "ctrl+f7", ExtraConfig: ""},
			{Prompt: "", HotKey: "ctrl+f8", ExtraConfig: ""},
			{Prompt: "", HotKey: "", ExtraConfig: ""},
			{Prompt: "", HotKey: "", ExtraConfig: ""},
		},
		HotKeyHook:  false,
		DEBUG:       false,
	}
}

func usage() {
	name := filepath.Base(os.Args[0])
	fmt.Fprintf(os.Stderr, `用法: %s [选项]

此程序为基于 LLM 的文本处理工具，支持通过 HotKeyConfig 数组自定义提示词与热键对（默认 10 组，支持用户在配置中新增任意数量）

选项:
[API 端点配置]
  -config <path>
        配置文件 JSON
  -api-endpoint <string>
  -token <string>
  -model <string>
  -temperature <float>
        默认温度为 "0"
  -max-tokens <int>
  -text-path <string>
        默认返回字段: choices[0].message.content
  -extra-config <string>
        解析自定义请求字段并合并到向 API 端点发送的请求中，必须填写转义字符串，否则将无法解析。
        默认: ""
        示例:
          "{\"verbosity\":\"low\"}"
          一个 JSON 格式的转义后字符串，允许使用数组。
          将会在请求体 payload 中加入根字段 verbosity。
          若存在同名字段，-extra-config 中的字段优先级高于预设字段。

[热键配置]
  HotKeyConfig 由于较复杂，暂不支持命令行输入，请到配置文件中以 JSON 数组形式进行配置。

  支持更细粒度的 ExtraConfig 字段配置，用法与根字段 ExtraConfig 一致，但优先级更高。
  支持使用 APIEndpoint、Token、TEXTPath 三个指定字段对 API 端点配置进行覆盖，仅在当前 Prompt 下生效。
  支持使用字段空值来清除已有字段，将会在请求时自动移除该字段，支持递归处理。

  JSON 配置示例：新增字段、删除字段、修改 API 端点。
  "HotKeyConfig": [
    {
      "Prompt": "Please translate the following text into English:",
      "HotKey": "ctrl+f1",
      "ExtraConfig": "{\"verbosity\":\"low\"}"
    },
    {
      "Prompt": "Please translate the following text into Chinese:",
      "HotKey": "ctrl+f2",
      "ExtraConfig": "{\"max_tokens\":,\"verbosity":\"\"}"
    },
    {
      "Prompt": "Extract keywords:",
      "HotKey": "ctrl+f3",
      "ExtraConfig": "{\"APIEndpoint\":\"https://example/api\",\"Token\":\"sk-override\",\"TEXTPath\":\"choices[0].text\",\"max_tokens\":2000}"
    }
  ]

  支持的热键键名与写法（大小写不敏感；修饰键与主键用 '+' 连接，例如 "ctrl+numpad1"）:
    1. 修饰键: ctrl, alt, shift, win （别名：control, menu, meta, super）
    2. 顶排数字键（top-row）: 0 1 2 3 4 5 6 7 8 9  （示例: "ctrl+1" 表示顶排数字 1）
    3. 字母键: a..z （示例: "ctrl+a"）
    4. 功能键: F1..F24 （示例: "ctrl+F5"）
    5. 命名键: esc/escape, enter/return, space, tab, backspace, insert, delete, home, end, pageup, pagedown, left, up, right, down
    6. 小键盘数字（建议写法）: numpad0..numpad9（同义别名: num0..num9, kp0..kp9）。示例: "ctrl+numpad1" 或 "ctrl+num1"
    7. 小键盘运算键（请使用别名，不要在 token 内使用字面 '+' 或 '-'）:
       加号（NumPad +）: add, plus, kpadd   （示例: "ctrl+add"）
       减号（NumPad -）: subtract, minus, kpsubtract   （示例: "alt+subtract"）
    8. 语法注意:
       '+' 字符用于分隔修饰键与主键；不要把 '+' 或 '-' 写入单个 token（例如请勿使用 "numpad+" 或 "numpad-"）。
       NumLock 状态可能影响小键盘按键在系统层面发出的虚拟键（VK）。
       为了得到一致行为，建议启用 NumLock；若需在 NumLock=off 时支持，请绑定相应的导航键名（如 "home","end","left" 等）。

[网络请求配置]
  -request-timeout <int>
        请求超时秒数（默认 30）
  -max-retry <int>
        上传最大重试次数（默认 3）
  -retry-base-delay <float>
        重试基准延迟秒（默认 0.5）
  -enable-http2 <true|false>
        是否启用 HTTP/2（默认开启）
  -verify-ssl <true|false>
        是否验证 HTTPS 证书（默认开启）。设置为 false 时会跳过 TLS 证书验证（不安全）。

[剪贴板配置]
  -clipboard-timeout <int>
        复制后等待剪贴板内容出现的超时时间（单位毫秒，默认 1000）

[DEBUG 配置]
  -debug <true|false>

示例:
  %s -config config.json
  %s -api-endpoint https://api.example/v1/chat -token sk-xxx

说明:
 - 配置优先级：命令行标志 > 配置文件 > 默认值
 - TEXTPath 使用点分法并支持方括号索引（例如 data.items[0].value）

`, name, name, name)
}

func loadConfig(path string) (Config, error) {
	cfg := defaultConfig()
	if path == "" {
		return cfg, nil
	}
	data, err := os.ReadFile(path)
	if err != nil {
		return cfg, err
	}
	if err := json.Unmarshal(data, &cfg); err != nil {
		return cfg, err
	}
	return cfg, nil
}

func saveDefaultConfig(path string) error {
	c := defaultConfig()
	out := struct {
		APIEndpoint      string        `json:"APIEndpoint"`
		Token            string        `json:"Token"`
		Model            string        `json:"Model"`
		Temperature      float64       `json:"Temperature"`
		MaxTokens        int           `json:"Max_Tokens"`
		TEXTPath         string        `json:"TEXTPath"`
		ExtraConfig      string        `json:"ExtraConfig"`
		RequestTimeout   int           `json:"RequestTimeout"`
		MaxRetry         int           `json:"MaxRetry"`
		RetryBaseDelay   float64       `json:"RetryBaseDelay"`
		EnableHTTP2      bool          `json:"EnableHTTP2"`
		VerifySSL        bool          `json:"VerifySSL"`
		ClipboardTimeout int           `json:"ClipboardTimeout"`
		HotKeyConfig     []HotKeyEntry `json:"HotKeyConfig"`
		HotKeyHook       bool          `json:"HotKeyHook"`
		DEBUG            bool          `json:"DEBUG"`
	}{
		APIEndpoint:      c.APIEndpoint,
		Token:            c.Token,
		Model:            c.Model,
		Temperature:      c.Temperature,
		MaxTokens:        c.MaxTokens,
		TEXTPath:         c.TEXTPath,
		ExtraConfig:      c.ExtraConfig,
		RequestTimeout:   c.RequestTimeout,
		MaxRetry:         c.MaxRetry,
		RetryBaseDelay:   c.RetryBaseDelay,
		EnableHTTP2:      c.EnableHTTP2,
		VerifySSL:        c.VerifySSL,
		ClipboardTimeout: c.ClipboardTimeout,
		HotKeyHook:       c.HotKeyHook,
		HotKeyConfig:     c.HotKeyConfig,
		DEBUG:            c.DEBUG,
	}
	b, err := json.MarshalIndent(out, "", "  ")
	if err != nil {
		return err
	}
	return os.WriteFile(path, b, 0644)
}

func mergeFlags(c *Config) {
	getStr := func(key string) (string, bool) {
		if p, ok := flagOverrides[key]; ok && p != nil && *p != "" {
			return *p, true
		}
		return "", false
	}
	if v, ok := getStr("api-endpoint"); ok { c.APIEndpoint = v }
	if v, ok := getStr("token"); ok { c.Token = v }
	if v, ok := getStr("model"); ok { c.Model = v }
	if v, ok := getStr("temperature"); ok {
		if f, err := strconv.ParseFloat(v, 64); err == nil { c.Temperature = f }
	}
	if v, ok := getStr("max-tokens"); ok {
		if n, err := strconv.Atoi(v); err == nil { c.MaxTokens = n }
	}
	if v, ok := getStr("text-path"); ok { c.TEXTPath = v }
	if v, ok := getStr("extra-config"); ok { c.ExtraConfig = v }
	// network
	if v, ok := getStr("request-timeout"); ok {
		if n, err := strconv.Atoi(v); err == nil { c.RequestTimeout = n }
	}
	if v, ok := getStr("max-retry"); ok {
		if n, err := strconv.Atoi(v); err == nil { c.MaxRetry = n }
	}
	if v, ok := getStr("retry-base-delay"); ok {
		if f, err := strconv.ParseFloat(v, 64); err == nil { c.RetryBaseDelay = f }
	}
	if v, ok := getStr("enable-http2"); ok {
		l := strings.ToLower(v)
		c.EnableHTTP2 = (l == "1" || l == "true" || l == "yes")
	}
	if v, ok := getStr("verify-ssl"); ok {
		l := strings.ToLower(v)
		c.VerifySSL = (l == "1" || l == "true" || l == "yes")
	}
	if v, ok := getStr("debug"); ok {
		l := strings.ToLower(v)
		c.DEBUG = (l == "1" || l == "true" || l == "yes")
	}
	if v, ok := getStr("hotkeyhook"); ok {
		l := strings.ToLower(v)
		c.HotKeyHook = (l == "1" || l == "true" || l == "yes")
	}
	if v, ok := getStr("clipboard-timeout"); ok {
	    if n, err := strconv.Atoi(v); err == nil {
	        c.ClipboardTimeout = n
	    }
	}
}

func initHTTPClient() {
	tr := &http.Transport{
		MaxIdleConns:        100,
		MaxIdleConnsPerHost: 100,
		IdleConnTimeout:     90 * time.Second,
	}
	if !cfg.VerifySSL {
		tr.TLSClientConfig = &tls.Config{InsecureSkipVerify: true}
	}
	if cfg.EnableHTTP2 {
		_ = http2.ConfigureTransport(tr)
	}
	httpTransport = tr
	httpClient = &http.Client{
		Transport: tr,
		Timeout: time.Duration(cfg.RequestTimeout) * time.Second,
	}
}

func parseHotkey(s string) (uint32, uint32, error) {
	if s == "" {
		return 0, 0, fmt.Errorf("empty key")
	}
	parts := strings.Split(s, "+")
	for i := range parts {
		parts[i] = strings.TrimSpace(strings.ToLower(parts[i]))
	}
	var mod uint32 = 0
	var keyToken string
	if len(parts) == 1 {
		keyToken = parts[0]
	} else {
		keyToken = parts[len(parts)-1]
		for _, p := range parts[:len(parts)-1] {
			switch p {
			case "alt", "menu":
				mod |= 0x0001 // MOD_ALT
			case "ctrl", "control":
				mod |= 0x0002 // MOD_CONTROL
			case "shift":
				mod |= 0x0004 // MOD_SHIFT
			case "win", "meta", "super":
				mod |= 0x0008 // MOD_WIN
			default:
			}
		}
	}
	// single char letters
	if len(keyToken) == 1 {
		ch := keyToken[0]
		if ch >= 'a' && ch <= 'z' {
			return mod, uint32(ch - 'a' + 'A'), nil
		}
		if ch >= '0' && ch <= '9' {
			// top-row digits (VK_0..VK_9 are 0x30..0x39 which match ASCII '0'..'9')
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
	}
	if strings.HasPrefix(keyToken, "f") {
		nStr := strings.TrimPrefix(keyToken, "f")
		if n, err := strconv.Atoi(nStr); err == nil && n >= 1 && n <= 24 {
			return mod, 0x70 + uint32(n-1), nil
		}
	}

	// numpad aliases: support multiple common forms
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

	named := map[string]uint32{
		"tab":       0x09,
		"backspace": 0x08,
		"insert":    0x2D,
		"delete":    0x2E,
		"home":      0x24,
		"end":       0x23,
		"pageup":    0x21,
		"pagedown":  0x22,
		"left":      0x25,
		"up":        0x26,
		"right":     0x27,
		"down":      0x28,
	}
	if v, ok := named[keyToken]; ok {
		return mod, v, nil
	}
	if len(keyToken) == 1 {
		return mod, uint32(strings.ToUpper(keyToken)[0]), nil
	}
	return 0, 0, fmt.Errorf("unsupported key token: %s", s)
}

// registerHotkeys registers the provided map(id->spec). It only registers keys that are non-empty and returns a map[id]spec registered.
func registerHotkeys(specs map[int]string) error {
	type entry struct {
		id  int
		mod uint32
		vk  uint32
		spec string
	}
	var entries []entry
	for id, spec := range specs {
		if strings.TrimSpace(spec) == "" {
			continue
		}
		mod, vk, err := parseHotkey(spec)
		if err != nil {
			return fmt.Errorf("invalid hotkey '%s' for id=%d: %v", spec, id, err)
		}
		entries = append(entries, entry{id: id, mod: mod, vk: vk, spec: spec})
	}

	if len(entries) == 0 {
		// nothing to register
		return nil
	}

	errCh := make(chan error, 1)
	go func() {
		runtime.LockOSThread()
		defer runtime.UnlockOSThread()
		user32 := syscall.NewLazyDLL("user32.dll")
		procRegisterHotKey := user32.NewProc("RegisterHotKey")
		procUnregisterHotKey := user32.NewProc("UnregisterHotKey")
		procGetMessageW := user32.NewProc("GetMessageW")

		for _, e := range entries {
			r, _, _ := procRegisterHotKey.Call(
				0,
				uintptr(e.id),
				uintptr(e.mod),
				uintptr(e.vk),
			)
			if r == 0 {
				// unregister previous
				for _, pe := range entries {
					if pe.id == e.id {
						break
					}
					procUnregisterHotKey.Call(0, uintptr(pe.id))
				}
				errCh <- fmt.Errorf("RegisterHotKey failed for '%s' id=%d", e.spec, e.id)
				return
			}
			if cfg.DEBUG {
				fmt.Printf("[hotkey] registered id=%d spec=%s\n", e.id, e.spec)
			}
		}
		// signal success
		errCh <- nil

		var msg struct {
			Hwnd    uintptr
			Message uint32
			WParam  uintptr
			LParam  uintptr
			Time    uint32
			Pt_x    int32
			Pt_y    int32
		}
		const WM_HOTKEY = 0x0312
		for {
			ret, _, _ := procGetMessageW.Call(uintptr(unsafe.Pointer(&msg)), 0, 0, 0)
			if int32(ret) == -1 {
				fmt.Println("[hotkey] GetMessageW error; exiting")
				return
			}
			if msg.Message == WM_HOTKEY {
				id := int(msg.WParam)
				if cfg.DEBUG {
					fmt.Printf("[hotkey] WM_HOTKEY id=%d\n", id)
				}
				handleHotkey(id)
			}
		}
	}()

	select {
	case err := <-errCh:
		return err
	case <-time.After(2 * time.Second):
		return fmt.Errorf("timeout registering hotkeys")
	}
}

var (
	// low-level hook globals
	llHookHandle         uintptr
	llHotkeyTable        map[uint32][]struct{ id int; mod uint32 }
	llBlockedMutex       sync.Mutex
	llBlocked            map[uint32]bool
	hotkeyEventCh        chan int
	procCallNextHookEx   *syscall.LazyProc
	procGetAsyncKeyState *syscall.LazyProc
)

const (
	WH_KEYBOARD_LL    = 13
	LLKHF_INJECTED    = 0x00000010
	WM_KEYDOWN        = 0x0100
	WM_KEYUP          = 0x0101
	WM_SYSKEYDOWN     = 0x0104
	WM_SYSKEYUP       = 0x0105
	VK_CONTROL        = 0x11
	VK_MENU           = 0x12
	VK_SHIFT          = 0x10
	VK_LWIN           = 0x5B
	VK_RWIN           = 0x5C

	// Numpad keys and common numpad operators
	VK_NUMPAD0        = 0x60
	VK_NUMPAD1        = 0x61
	VK_NUMPAD2        = 0x62
	VK_NUMPAD3        = 0x63
	VK_NUMPAD4        = 0x64
	VK_NUMPAD5        = 0x65
	VK_NUMPAD6        = 0x66
	VK_NUMPAD7        = 0x67
	VK_NUMPAD8        = 0x68
	VK_NUMPAD9        = 0x69
	VK_ADD            = 0x6B
	VK_SUBTRACT       = 0x6D
)

// startLowLevelKeyboardHook installs a WH_KEYBOARD_LL hook and dispatches matched hotkeys.
// It runs the hook and a message loop on a locked OS thread.
func startLowLevelKeyboardHook(specs map[int]string) error {
	// build lookup table vk -> []{id, mod}
	llHotkeyTable = make(map[uint32][]struct{ id int; mod uint32 })
	for id, spec := range specs {
		if strings.TrimSpace(spec) == "" {
			continue
		}
		mod, vk, err := parseHotkey(spec)
		if err != nil {
			return fmt.Errorf("invalid hotkey '%s' for id=%d: %v", spec, id, err)
		}
		llHotkeyTable[vk] = append(llHotkeyTable[vk], struct{ id int; mod uint32 }{id: id, mod: mod})
	}

	hotkeyEventCh = make(chan int, 16)
	llBlocked = make(map[uint32]bool)

	errCh := make(chan error, 1)
	go func() {
		runtime.LockOSThread()
		defer runtime.UnlockOSThread()

		user32 := syscall.NewLazyDLL("user32.dll")
		procSetWindowsHookEx := user32.NewProc("SetWindowsHookExW")
		procUnhookWindowsHookEx := user32.NewProc("UnhookWindowsHookEx")
		procGetMessageW := user32.NewProc("GetMessageW")
		procCallNextHookEx = user32.NewProc("CallNextHookEx")
		procGetAsyncKeyState = user32.NewProc("GetAsyncKeyState")

		cb := syscall.NewCallback(lowLevelKeyboardProc)
		h, _, e := procSetWindowsHookEx.Call(
			uintptr(WH_KEYBOARD_LL),
			cb,
			0,
			0,
		)
		if h == 0 {
			errCh <- fmt.Errorf("SetWindowsHookExW failed: %v", e)
			return
		}
		llHookHandle = h
		if cfg.DEBUG {
			fmt.Printf("[hotkey] low-level hook installed handle=0x%x\n", llHookHandle)
		}
		errCh <- nil

		var msg struct {
			Hwnd    uintptr
			Message uint32
			WParam  uintptr
			LParam  uintptr
			Time    uint32
			Pt_x    int32
			Pt_y    int32
		}
		for {
			ret, _, _ := procGetMessageW.Call(uintptr(unsafe.Pointer(&msg)), 0, 0, 0)
			if int32(ret) == -1 {
				fmt.Println("[hotkey] GetMessageW error; exiting")
				break
			}
			// loop until Unhook -> then exit when GetMessageW fails or thread ends
		}
		// cleanup
		procUnhookWindowsHookEx.Call(llHookHandle)
		if cfg.DEBUG {
			fmt.Println("[hotkey] low-level hook uninstalled")
		}
	}()

	// dispatcher that calls handleHotkey asynchronously
	go func() {
		for id := range hotkeyEventCh {
			go handleHotkey(id)
		}
	}()

	select {
	case err := <-errCh:
		return err
	case <-time.After(2 * time.Second):
		return fmt.Errorf("timeout installing low-level keyboard hook")
	}
}

// lowLevelKeyboardProc is the callback invoked by Windows for keyboard events.
// It returns 1 to swallow the event, or calls CallNextHookEx to pass it on.
func lowLevelKeyboardProc(nCode int, wParam uintptr, lParam uintptr) uintptr {
	if nCode < 0 {
		// pass through
		ret, _, _ := procCallNextHookEx.Call(0, uintptr(nCode), wParam, lParam)
		return ret
	}
	k := (*struct {
		VkCode     uint32
		ScanCode   uint32
		Flags      uint32
		Time       uint32
		DwExtraInfo uintptr
	})(unsafe.Pointer(lParam))

	// ignore injected events (so our simulateCopy/simulatePaste are not swallowed)
	if (k.Flags & LLKHF_INJECTED) != 0 {
		ret, _, _ := procCallNextHookEx.Call(0, uintptr(nCode), wParam, lParam)
		return ret
	}

	msg := uint32(wParam)
	switch msg {
	case WM_KEYDOWN, WM_SYSKEYDOWN:
		vk := k.VkCode
		entries, ok := llHotkeyTable[vk]
		if !ok {
			break
		}
		// check modifier state for each candidate
		for _, e := range entries {
			if modsMatch(e.mod) {
				// mark blocked so we also swallow the corresponding KEYUP
				llBlockedMutex.Lock()
				llBlocked[vk] = true
				llBlockedMutex.Unlock()
				// signal handler (non-blocking)
				select {
				case hotkeyEventCh <- e.id:
				default:
				}
				// swallow the event
				return 1
			}
		}
	case WM_KEYUP, WM_SYSKEYUP:
		vk := k.VkCode
		llBlockedMutex.Lock()
		blocked := llBlocked[vk]
		if blocked {
			delete(llBlocked, vk)
			llBlockedMutex.Unlock()
			return 1
		}
		llBlockedMutex.Unlock()
	}

	// default: pass to next hook
	ret, _, _ := procCallNextHookEx.Call(0, uintptr(nCode), wParam, lParam)
	return ret
}

// modsMatch checks whether current keyboard modifier state satisfies the required mod mask.
// Uses GetAsyncKeyState to check current state of Ctrl/Alt/Shift/Win.
func modsMatch(required uint32) bool {
	// helper to test whether a virtual-key is currently down
	isDown := func(vk int) bool {
		if procGetAsyncKeyState == nil {
			return false
		}
		r, _, _ := procGetAsyncKeyState.Call(uintptr(vk))
		return int32(r)&0x8000 != 0
	}
	// MOD_ALT (0x0001)
	if required&0x0001 != 0 {
		if !isDown(VK_MENU) {
			return false
		}
	} else {
		// if not required, it's okay whether it's pressed or not
	}
	// MOD_CONTROL (0x0002)
	if required&0x0002 != 0 {
		if !isDown(VK_CONTROL) {
			return false
		}
	}
	// MOD_SHIFT (0x0004)
	if required&0x0004 != 0 {
		if !isDown(VK_SHIFT) {
			return false
		}
	}
	// MOD_WIN (0x0008) -- consider either LWIN or RWIN
	if required&0x0008 != 0 {
		if !(isDown(VK_LWIN) || isDown(VK_RWIN)) {
			return false
		}
	}
	return true
}

func getPromptByID(id int) string {
	if id < 1 || id > len(cfg.HotKeyConfig) {
		return ""
	}
	return cfg.HotKeyConfig[id-1].Prompt
}

func handleHotkey(id int) {
	prompt := getPromptByID(id)
	if strings.TrimSpace(prompt) == "" {
		if cfg.DEBUG {
			fmt.Printf("[hotkey] prompt empty for id=%d, ignoring\n", id)
		}
		return
	}

	// Copy selected text into variable using copyText()
	copied, err := copyText()
	if err != nil {
		if cfg.DEBUG {
			fmt.Printf("[copy] copyText error: %v\n", err)
		}
		return
	}
	if strings.TrimSpace(copied) == "" {
		if cfg.DEBUG {
			fmt.Println("[copy] clipboard empty, nothing to do")
		}
		return
	}

	// per-entry ExtraConfig overrides: parse and extract APIEndpoint/Token/TEXTPath if present
	apiEndpoint := ""
	token := ""
	textPath := ""
	var perEntryExtra map[string]interface{}
	if id >= 1 && id <= len(cfg.HotKeyConfig) {
		ec := strings.TrimSpace(cfg.HotKeyConfig[id-1].ExtraConfig)
		if ec != "" {
			perEntryExtra = make(map[string]interface{})
			if err := json.Unmarshal([]byte(ec), &perEntryExtra); err != nil {
				if cfg.DEBUG {
					fmt.Printf("[hotkey] invalid ExtraConfig JSON for id=%d: %v\n", id, err)
				}
				perEntryExtra = nil
			} else {
				// extract special keys if present (each checked individually) and remove them from perEntryExtra
				if v, ok := perEntryExtra["APIEndpoint"]; ok {
					if s, ok2 := v.(string); ok2 && strings.TrimSpace(s) != "" {
						apiEndpoint = s
					}
					delete(perEntryExtra, "APIEndpoint")
				}
				if v, ok := perEntryExtra["Token"]; ok {
					if s, ok2 := v.(string); ok2 && strings.TrimSpace(s) != "" {
						token = s
					}
					delete(perEntryExtra, "Token")
				}
				if v, ok := perEntryExtra["TEXTPath"]; ok {
					if s, ok2 := v.(string); ok2 && strings.TrimSpace(s) != "" {
						textPath = s
					}
					delete(perEntryExtra, "TEXTPath")
				}
			}
		}
	}

	// build request body
	reqBodyMap := make(map[string]interface{})
	if cfg.Model != "" {
		reqBodyMap["model"] = cfg.Model
	}
	// messages
	messages := []map[string]string{}
	messages = append(messages, map[string]string{
		"role":    "developer",
		"content": prompt,
	})
	messages = append(messages, map[string]string{
		"role":    "user",
		"content": copied,
	})
	reqBodyMap["messages"] = messages
	// optional fields
	if cfg.MaxTokens > 0 {
		reqBodyMap["max_tokens"] = cfg.MaxTokens
	}
	// temperature (always present)
	reqBodyMap["temperature"] = cfg.Temperature

	// merge ExtraConfig precedence: per-entry > global extraConfigMap
	mergedExtra := make(map[string]interface{})
	if extraConfigMap != nil {
		for k, v := range extraConfigMap {
			mergedExtra[k] = v
		}
	}
	if perEntryExtra != nil {
		for k, v := range perEntryExtra {
			mergedExtra[k] = v
		}
	}
	for k, v := range mergedExtra {
		reqBodyMap[k] = v
	}

	// send request with retries (use per-entry overrides if provided)
	resBody, err := sendRequestWithRetry(reqBodyMap, apiEndpoint, token)
	if err != nil {
		if cfg.DEBUG { fmt.Printf("[request] failed after retries: %v\n", err) }
		return
	}
	// extract text by TEXTPath (use per-entry override if provided)
	extracted := extractTextFromResponse(resBody, textPath)
	if extracted == "" {
		if cfg.DEBUG { fmt.Println("[extract] extracted empty") }
		return
	}
	// paste extracted using pasteText (template B) which restores clipboard at end
	if err := pasteText(extracted); err != nil {
		if cfg.DEBUG { fmt.Printf("[paste] error: %v\n", err) }
	}
}

// simulateCopy simulates Ctrl+C
func simulateCopy() error {
	kb, err := keybd_event.NewKeyBonding()
	if err != nil {
		return err
	}
	kb.HasCTRL(true)
	// VK_C constant provided by lib; use uppercase 'C' code
	kb.SetKeys(keybd_event.VK_C)
	if err := kb.Launching(); err != nil {
		return err
	}
	return nil
}

 // simulatePaste simulates Ctrl+V
 func simulatePaste() error {
 	kb, err := keybd_event.NewKeyBonding()
 	if err != nil {
 		return err
 	}
 	kb.HasCTRL(true)
 	kb.SetKeys(keybd_event.VK_V)
 	if err := kb.Launching(); err != nil {
 		return err
 	}
 	return nil
 }

 // copyText: backup clipboard, simulate Ctrl+C, wait+read clipboard (with retries), restore clipboard, return copied text.
func copyText() (string, error) {
    // backup original clipboard
    orig, _ := clipboard.ReadAll()

    // ensure orig is restored on all exit paths
    defer func() {
        // small delay to allow any pending clipboard operations to finish
        time.Sleep(150 * time.Millisecond)
        // restore with retries
        for i := 0; i < 5; i++ {
            if err := clipboard.WriteAll(orig); err == nil {
                break
            }
            time.Sleep(50 * time.Millisecond)
        }
    }()

    // try clearing the clipboard multiple times, but do not treat failure as a fatal error.
    for i := 0; i < 5; i++ {
        if err := clipboard.WriteAll(""); err == nil {
            break
        } else {
            if cfg.DEBUG {
                fmt.Printf("[copy] failed to clear clipboard: %v\n", err)
            }
            time.Sleep(50 * time.Millisecond)
        }
    }

    // give the system some time to complete the clipboard clearing operation
    time.Sleep(50 * time.Millisecond)

    // simulate Ctrl+C to copy selected text
    if err := simulateCopy(); err != nil {
        return "", err
    }

    // poll clipboard until it becomes non-empty or timeout
    timeout := time.After(time.Duration(cfg.ClipboardTimeout) * time.Millisecond)
    ticker  := time.NewTicker(50 * time.Millisecond)
    defer ticker.Stop()
    for {
        select {
        case <-timeout:
            return "", fmt.Errorf("timeout waiting for clipboard after Ctrl+C")
        case <-ticker.C:
            res, err := clipboard.ReadAll()
            if err != nil {
                // transient read error; retry
                if cfg.DEBUG {
                    fmt.Printf("[copy] clipboard read error: %v\n", err)
                }
                continue
            }
            // 仅判断非空：即使与 orig 相同也接受
            if strings.TrimSpace(res) != "" {
                return res, nil
            }
        }
    }
}

 // pasteText: backup clipboard, write text with retries, simulate Ctrl+V, restore clipboard.
 func pasteText(text string) error {
 	orig, _ := clipboard.ReadAll()
 	// ensure restore on all exit paths
 	defer func() {
 		time.Sleep(120 * time.Millisecond)
 		for i := 0; i < 5; i++ {
 			if err := clipboard.WriteAll(orig); err == nil {
 				break
 			}
 			time.Sleep(50 * time.Millisecond)
 		}
 	}()

 	// try writing to clipboard with retries
 	var lastErr error
 	for i := 0; i < 5; i++ {
 		if err := clipboard.WriteAll(text); err == nil {
 			lastErr = nil
 			break
 		} else {
 			lastErr = err
 			if cfg.DEBUG {
 				fmt.Printf("[paste] clipboard write attempt %d failed: %v\n", i+1, err)
 			}
 			time.Sleep(50 * time.Millisecond)
 		}
 	}
 	if lastErr != nil {
 		return fmt.Errorf("failed to write clipboard: %v", lastErr)
 	}

 	// small sleep to allow clipboard to be ready
 	time.Sleep(80 * time.Millisecond)

 	// simulate paste
 	if err := simulatePaste(); err != nil {
 		return err
 	}
 	return nil
 }

func pasteTextPreserveClipboard(text, orig string) error {
	// write desired text
	if err := clipboard.WriteAll(text); err != nil {
		return err
	}
	time.Sleep(80 * time.Millisecond)
	if err := simulatePaste(); err != nil {
		// attempt restore
		_ = clipboard.WriteAll(orig)
		return err
	}
	// small delay then restore
	time.Sleep(150 * time.Millisecond)
	_ = clipboard.WriteAll(orig)
	return nil
}

// stripEmptyFields recursively removes "empty" values from a map[string]interface{}.
// Definition of "empty":
//  - nil
//  - empty string (after TrimSpace)
//  - empty slice/array (length == 0)
//  - empty map (length == 0)
// Note: numeric zero (0) and boolean false are considered valid values and are preserved.
func stripEmptyFields(m map[string]interface{}) map[string]interface{} {
	if m == nil {
		return nil
	}
	out := make(map[string]interface{}, len(m))
	for k, v := range m {
		if v == nil {
			continue
		}
		switch vv := v.(type) {
		case string:
			if strings.TrimSpace(vv) == "" {
				continue
			}
			out[k] = vv
		case map[string]interface{}:
			cleaned := stripEmptyFields(vv)
			if len(cleaned) > 0 {
				out[k] = cleaned
			}
		case []interface{}:
			arr := make([]interface{}, 0, len(vv))
			for _, el := range vv {
				switch elv := el.(type) {
				case map[string]interface{}:
					cleanedEl := stripEmptyFields(elv)
					if len(cleanedEl) > 0 {
						arr = append(arr, cleanedEl)
					}
				case []interface{}:
					// recursively clean nested slices
					cleaned := cleanInterface(elv)
					if a, ok := cleaned.([]interface{}); ok && len(a) > 0 {
						arr = append(arr, a)
					}
				case string:
					if strings.TrimSpace(elv) != "" {
						arr = append(arr, elv)
					}
				default:
					// keep numbers, bools, and other types
					arr = append(arr, elv)
				}
			}
			if len(arr) > 0 {
				out[k] = arr
			}
		default:
			// keep numbers, bools, and other types as-is
			out[k] = v
		}
	}
	return out
}

// cleanInterface cleans arbitrary interface{} values (maps/slices/primitives) and returns
// either a cleaned value or nil when the value should be treated as "empty".
func cleanInterface(val interface{}) interface{} {
	if val == nil {
		return nil
	}
	switch t := val.(type) {
	case map[string]interface{}:
		cleaned := stripEmptyFields(t)
		if len(cleaned) == 0 {
			return nil
		}
		return cleaned
	case []interface{}:
		arr := make([]interface{}, 0, len(t))
		for _, el := range t {
			c := cleanInterface(el)
			if c == nil {
				continue
			}
			switch cc := c.(type) {
			case []interface{}:
				if len(cc) > 0 {
					arr = append(arr, cc)
				}
			case map[string]interface{}:
				if len(cc) > 0 {
					arr = append(arr, cc)
				}
			default:
				// keep primitives (strings already filtered by cleanInterface)
				arr = append(arr, cc)
			}
		}
		if len(arr) == 0 {
			return nil
		}
		return arr
	case string:
		if strings.TrimSpace(t) == "" {
			return nil
		}
		return t
	default:
		// numbers, bools, other types: keep as-is
		return val
	}
}

func sendRequestWithRetry(body map[string]interface{}, apiEndpoint, token string) ([]byte, error) {
	endpoint := strings.TrimSpace(apiEndpoint)
	if endpoint == "" {
		endpoint = strings.TrimSpace(cfg.APIEndpoint)
	}
	if endpoint == "" {
		return nil, errors.New("API endpoint empty")
	}
	tries := 0
	delay := cfg.RetryBaseDelay
	var lastErr error
	for {
		tries++
		// clean request body by removing empty fields to avoid sending extraneous empty values
		cleanedBody := stripEmptyFields(body)
		b, err := json.Marshal(cleanedBody)
		if err != nil {
			return nil, err
		}
		if cfg.DEBUG {
			fmt.Printf("[request] attempt=%d payload=%s\n", tries, string(b))
		}
		req, err := http.NewRequest("POST", endpoint, bytes.NewBuffer(b))
		if err != nil {
			return nil, err
		}
		req.Header.Set("Content-Type", "application/json")
		localToken := token
		if strings.TrimSpace(localToken) == "" {
			localToken = cfg.Token
		}
		if localToken != "" {
			req.Header.Set("Authorization", "Bearer "+localToken)
		}
		req.Header.Set("User-Agent", "clip-hotkey-client/1.0")
		resp, err := httpClient.Do(req)
		if err != nil {
			lastErr = err
			if cfg.DEBUG { fmt.Printf("[request] error: %v\n", err) }
		} else {
			defer resp.Body.Close()
			resBody, _ := io.ReadAll(resp.Body)
			if resp.StatusCode >= 200 && resp.StatusCode < 300 {
				if cfg.DEBUG {
					fmt.Printf("[request] success status=%d body=%s\n", resp.StatusCode, string(resBody))
				}
				return resBody, nil
			}
			lastErr = fmt.Errorf("status %d: %s", resp.StatusCode, string(resBody))
			if cfg.DEBUG {
				fmt.Printf("[request] non-200 status: %v\n", lastErr)
			}
		}
		if tries >= cfg.MaxRetry {
			break
		}
		// sleep then retry
		time.Sleep(time.Duration(delay * float64(time.Second)))
		delay *= 2
	}
	return nil, lastErr
}

func extractTextFromResponse(body []byte, textPath string) string {
	var root interface{}
	if err := json.Unmarshal(body, &root); err != nil {
		if cfg.DEBUG { fmt.Printf("[extract] json parse error: %v\n", err) }
		return ""
	}
	// use provided textPath if set, otherwise fallback to cfg.TEXTPath
	if strings.TrimSpace(textPath) == "" {
		textPath = cfg.TEXTPath
	}
	// try TEXTPath if configured
	if textPath != "" {
		if v, ok := extractByPath(root, textPath); ok {
			return v
		}
	}
	// fallback: look for text top-level
	if m, ok := root.(map[string]interface{}); ok {
		if v, exists := m["text"]; exists {
			switch s := v.(type) {
			case string:
				return s
			default:
				b, _ := json.Marshal(s)
				return string(b)
			}
		}
		// any non-empty string at top-level
		for _, val := range m {
			if s, ok := val.(string); ok && s != "" {
				return s
			}
		}
	}
	return ""
}

// extractByPath copied/adapted logic: dot-separated with [i] indexes
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
			next, exists := m[key]
			if !exists {
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
	var key string
	if br == -1 {
		key = token
		return key, idxs, nil
	}
	key = token[:br]
	rest := token[br:]
	for len(rest) > 0 {
		if !strings.HasPrefix(rest, "[") {
			return "", nil, fmt.Errorf("invalid index syntax in %s", token)
		}
		closePos := strings.Index(rest, "]")
		if closePos == -1 {
			return "", nil, fmt.Errorf("missing closing ] in %s", token)
		}
		numStr := rest[1:closePos]
		if numStr == "" {
			return "", nil, fmt.Errorf("empty index in %s", token)
		}
		n, err := strconv.Atoi(numStr)
		if err != nil {
			return "", nil, fmt.Errorf("invalid index '%s' in %s", numStr, token)
		}
		idxs = append(idxs, n)
		rest = rest[closePos+1:]
	}
	return key, idxs, nil
}

func main() {
	flag.Usage = usage
	flag.StringVar(&flagConfigPath, "config", "", "config json path")
	flagOverrides["api-endpoint"] = flag.String("api-endpoint", "", "api endpoint")
	flagOverrides["token"] = flag.String("token", "", "token")
	flagOverrides["model"] = flag.String("model", "", "model")
	flagOverrides["temperature"] = flag.String("temperature", "", "temperature")
	flagOverrides["max-tokens"] = flag.String("max-tokens", "", "max tokens")
	flagOverrides["text-path"] = flag.String("text-path", "", "text path")
	flagOverrides["extra-config"] = flag.String("extra-config", "", "extra config json")
	// network
	flagOverrides["request-timeout"] = flag.String("request-timeout", "", "request timeout")
	flagOverrides["max-retry"] = flag.String("max-retry", "", "max retry")
	flagOverrides["retry-base-delay"] = flag.String("retry-base-delay", "", "retry base delay")
	flagOverrides["enable-http2"] = flag.String("enable-http2", "", "enable http2")
	flagOverrides["verify-ssl"] = flag.String("verify-ssl", "", "verify ssl")
	flagOverrides["clipboard-timeout"] = flag.String("clipboard-timeout", "", "clipboard timeout (ms)")
	flagOverrides["debug"] = flag.String("debug", "", "debug")
	flagOverrides["hotkeyhook"] = flag.String("hotkeyhook", "", "hotkeyhook (true|false)")

	help := flag.Bool("h", false, "help")
	flag.Parse()
	if *help {
		usage()
		return
	}

	// load config
	if flagConfigPath != "" {
		c, err := loadConfig(flagConfigPath)
		if err != nil {
			fmt.Printf("[main] failed load config %s: %v\n", flagConfigPath, err)
			os.Exit(1)
		}
		cfg = c
	} else {
		// if config.json exists load it
		if _, err := os.Stat("config.json"); err == nil {
			c, err := loadConfig("config.json")
			if err != nil {
				fmt.Printf("[main] failed load config.json: %v\n", err)
				os.Exit(1)
			}
			cfg = c
		} else if os.IsNotExist(err) {
			// no config -> if no flags provided, create default and exit
			anyFlag := false
			for _, p := range flagOverrides {
				if p != nil && *p != "" {
					anyFlag = true
					break
				}
			}
			if !anyFlag {
				if err := saveDefaultConfig("config.json"); err != nil {
					fmt.Printf("[main] failed create default config: %v\n", err)
					os.Exit(1)
				}
				fmt.Println("[main] default config.json created. Please edit it and re-run.")
				return
			}
			cfg = defaultConfig()
		} else {
			fmt.Printf("[main] stat config.json failed: %v\n", err)
			os.Exit(1)
		}
	}
	mergeFlags(&cfg)

	// parse ExtraConfig
	if cfg.ExtraConfig != "" {
		extraConfigMap = make(map[string]interface{})
		if err := json.Unmarshal([]byte(cfg.ExtraConfig), &extraConfigMap); err != nil {
			fmt.Printf("[main] invalid ExtraConfig JSON: %v\n", err)
			os.Exit(1)
		}
	}

	// init http client
	initHTTPClient()

	// prepare hotkey specs using HotKeyConfig array (id starts at 1)
	specs := map[int]string{}
	for i := 0; i < len(cfg.HotKeyConfig); i++ {
		entry := cfg.HotKeyConfig[i]
		// only register entries where BOTH Prompt and HotKey are non-empty after trimming
		if strings.TrimSpace(entry.Prompt) != "" && strings.TrimSpace(entry.HotKey) != "" {
			specs[i+1] = entry.HotKey
		}
	}

	if len(specs) == 0 {
		fmt.Println("[main] no prompts configured; nothing to register. Exiting.")
		return
	}

	// choose registration method based on HotKeyHook flag
	if cfg.HotKeyHook {
		if err := startLowLevelKeyboardHook(specs); err != nil {
			fmt.Printf("[main] failed install low-level hook: %v\n", err)
			os.Exit(1)
		}
	} else {
		if err := registerHotkeys(specs); err != nil {
			fmt.Printf("[main] failed register hotkeys: %v\n", err)
			os.Exit(1)
		}
	}

	// catch signals to exit cleanly
	sigCh := make(chan os.Signal, 1)
	signal.Notify(sigCh, os.Interrupt, syscall.SIGTERM)
	fmt.Println("[main] ready. Press configured hotkeys to invoke. Ctrl+C to exit.")
	<-sigCh
	fmt.Println("[main] exiting")
	// close idle connections
	if httpTransport != nil {
		httpTransport.CloseIdleConnections()
	}
}
