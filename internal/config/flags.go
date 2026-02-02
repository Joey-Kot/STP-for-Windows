package config

import (
	"flag"
	"fmt"
	"io"
	"strconv"
	"strings"
)

type CLIOptions struct {
	ConfigPath string
	ShowHelp   bool

	APIEndpoint               string
	Token                     string
	Model                     string
	Temperature               float64
	MaxTokens                 int
	TEXTPath                  string
	ExtraConfig               string
	RequestTimeout            int
	MaxRetry                  int
	RetryBaseDelay            float64
	EnableHTTP2               bool
	VerifySSL                 bool
	ClipboardTimeout          int
	RequestFailedNotification bool
	StopTaskHotkey            string
	HotKeyHook                bool
	DEBUG                     bool

	set map[string]bool
}

func ParseCLI(args []string, stderr io.Writer) (CLIOptions, error) {
	opts := CLIOptions{set: make(map[string]bool)}
	fs := flag.NewFlagSet("stp", flag.ContinueOnError)
	fs.SetOutput(stderr)
	fs.StringVar(&opts.ConfigPath, "config", "", "JSON path of config file")
	fs.StringVar(&opts.APIEndpoint, "api-endpoint", "", "api endpoint")
	fs.StringVar(&opts.Token, "token", "", "token")
	fs.StringVar(&opts.Model, "model", "", "model")
	fs.Float64Var(&opts.Temperature, "temperature", 0, "temperature")
	fs.IntVar(&opts.MaxTokens, "max-tokens", 0, "max tokens")
	fs.StringVar(&opts.TEXTPath, "text-path", "", "text path")
	fs.StringVar(&opts.ExtraConfig, "extra-config", "", "extra config")
	fs.IntVar(&opts.RequestTimeout, "request-timeout", 0, "request timeout")
	fs.IntVar(&opts.MaxRetry, "max-retry", 0, "max retry")
	fs.Float64Var(&opts.RetryBaseDelay, "retry-base-delay", 0, "retry base delay")
	fs.BoolVar(&opts.EnableHTTP2, "enable-http2", false, "enable http2")
	fs.BoolVar(&opts.VerifySSL, "verify-ssl", false, "verify ssl")
	fs.IntVar(&opts.ClipboardTimeout, "clipboard-timeout", 0, "clipboard timeout (ms)")
	fs.BoolVar(&opts.RequestFailedNotification, "request-failed-notification", false, "paste placeholder when failed/empty")
	fs.StringVar(&opts.StopTaskHotkey, "stop-task-hotkey", "", "global hotkey to cancel current task and clear queue")
	fs.BoolVar(&opts.HotKeyHook, "hotkeyhook", false, "hotkeyhook (true|false)")
	fs.BoolVar(&opts.DEBUG, "debug", false, "debug")
	fs.BoolVar(&opts.ShowHelp, "h", false, "help")

	if err := fs.Parse(args); err != nil {
		return opts, err
	}
	fs.Visit(func(f *flag.Flag) {
		opts.set[f.Name] = true
	})
	return opts, nil
}

func (o CLIOptions) AnyOverrideSet() bool {
	for k := range o.set {
		if k != "config" && k != "h" {
			return true
		}
	}
	return false
}

func (o CLIOptions) IsSet(name string) bool {
	return o.set[name]
}

func ApplyCLI(c *Config, o CLIOptions) {
	if o.IsSet("api-endpoint") {
		c.APIEndpoint = o.APIEndpoint
	}
	if o.IsSet("token") {
		c.Token = o.Token
	}
	if o.IsSet("model") {
		c.Model = o.Model
	}
	if o.IsSet("temperature") {
		c.Temperature = o.Temperature
	}
	if o.IsSet("max-tokens") {
		c.MaxTokens = o.MaxTokens
	}
	if o.IsSet("text-path") {
		c.TEXTPath = o.TEXTPath
	}
	if o.IsSet("extra-config") {
		c.ExtraConfig = o.ExtraConfig
	}
	if o.IsSet("request-timeout") {
		c.RequestTimeout = o.RequestTimeout
	}
	if o.IsSet("max-retry") {
		c.MaxRetry = o.MaxRetry
	}
	if o.IsSet("retry-base-delay") {
		c.RetryBaseDelay = o.RetryBaseDelay
	}
	if o.IsSet("enable-http2") {
		c.EnableHTTP2 = o.EnableHTTP2
	}
	if o.IsSet("verify-ssl") {
		c.VerifySSL = o.VerifySSL
	}
	if o.IsSet("clipboard-timeout") {
		c.ClipboardTimeout = o.ClipboardTimeout
	}
	if o.IsSet("request-failed-notification") {
		c.RequestFailedNotification = o.RequestFailedNotification
	}
	if o.IsSet("stop-task-hotkey") {
		c.StopTaskHotkey = o.StopTaskHotkey
	}
	if o.IsSet("hotkeyhook") {
		c.HotKeyHook = o.HotKeyHook
	}
	if o.IsSet("debug") {
		c.DEBUG = o.DEBUG
	}
}

func Usage(w io.Writer, program string) {
	fmt.Fprintf(w, `用法: %s [选项]

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

[任务控制配置]
  -request-failed-notification <true|false>
        开启后：请求失败粘贴 [request failed]，空结果粘贴 [empty result]（默认 false）
  -stop-task-hotkey <string>
        全局停止热键：取消当前请求并清空等待队列（默认空字符串表示不启用）

[DEBUG 配置]
  -debug <true|false>

示例:
  %s -config config.json
  %s -api-endpoint https://api.example/v1/chat -token sk-xxx

说明:
 - 配置优先级：命令行标志 > 配置文件 > 默认值
 - TEXTPath 使用点分法并支持方括号索引（例如 data.items[0].value）

`, program, program, program)
}

func ParseBoolString(s string) (bool, error) {
	s = strings.TrimSpace(strings.ToLower(s))
	if s == "" {
		return false, fmt.Errorf("empty bool")
	}
	if s == "1" || s == "yes" {
		return true, nil
	}
	if s == "0" || s == "no" {
		return false, nil
	}
	return strconv.ParseBool(s)
}
