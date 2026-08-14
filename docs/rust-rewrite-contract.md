# STP-for-Windows Rust 重写行为契约

本文档记录重写前 Go 实现冻结下来的兼容行为，作为当前 Rust 实现的回归测试和维护依据。除明确写明的 CLI 语法变化外，兼容目标以重写时固化的代码行为和黄金数据为准。

## 1. 交付边界

- 重写仅替换实现语言；程序仍为后台控制台程序，不增加 GUI、托盘、系统通知或逐字符输入。
- 当前唯一实现为 Rust，正式 Windows 产物名为 `stp.exe`。
- 仓库不再包含 Go 源码、`go.mod`、`go.sum` 或 Go 构建步骤。
- Rust 与 STT 不共享 crate。
- 禁止使用 `SendInput`。复制和粘贴只调用 `user32!keybd_event`。

## 2. 配置与 CLI

### 默认值

| 字段 | 默认值 |
|---|---|
| `APIEndpoint` | `""` |
| `Token` | `""` |
| `Model` | `""` |
| `Temperature` | `0.0` |
| `Max_Tokens` | `0` |
| `TEXTPath` | `"choices[0].message.content"` |
| `ExtraConfig` | `""` |
| `RequestTimeout` | `30` 秒 |
| `MaxRetry` | `3` 次总尝试 |
| `RetryBaseDelay` | `0.5` 秒 |
| `EnableHTTP2` | `true` |
| `VerifySSL` | `true` |
| `ClipboardTimeout` | `1000` 毫秒 |
| `ClipboardWriteDelay` | `80` 毫秒 |
| `ClipboardRestoreDelay` | `120` 毫秒 |
| `RequestFailedNotification` | `false` |
| `StopTaskHotkey` | `""` |
| `HotKeyHook` | `false` |
| `DEBUG` | `false` |

默认生成 10 个 `HotKeyConfig` 项：前 8 项的热键依次为 `ctrl+f1` 至 `ctrl+f8`，后 2 项热键为空；全部 `Prompt` 和 `ExtraConfig` 为空。

### JSON 兼容规则

- 先创建默认配置，再覆盖 JSON 中出现且类型正确的字段。
- 缺失字段保留默认值；未知字段忽略。
- 顶层 JSON `null` 保留全部默认值。
- 普通标量字段为 `null` 时保留该字段默认值。
- `HotKeyConfig: null` 清空热键数组；数组中的 `null` 项视为全空条目。
- 类型不匹配或 JSON 语法错误属于配置错误。

### 配置选择与覆盖顺序

1. `--config` 指定的文件。标准 CLI 解析器拒绝空路径值；配置选择函数若收到空路径则使用默认配置。
2. 当前目录的 `config.json`。
3. 没有配置且没有任何覆盖参数时，生成默认 `config.json` 并以状态码 `0` 退出。
4. 没有配置但存在覆盖参数时，从默认配置开始。
5. CLI 中显式出现的字段覆盖配置文件；未出现的字段不覆盖，尤其是布尔字段。

CLI 使用标准双横线长参数。旧式单横线长参数不兼容。布尔参数需要显式值，例如 `--verify-ssl false` 或 `--enable-http2=true`。字符串参数的空值不计为有效覆盖。

退出码：帮助和默认配置生成成功为 `0`，配置或运行错误为 `1`，CLI 解析错误为 `2`。

## 3. 请求构造

- 全局和单条 `ExtraConfig` 必须是 JSON 对象；空白字符串与 JSON `null` 视为无扩展配置。
- 合并优先级为：单条扩展字段 > 全局扩展字段 > 内置字段。
- 只有单条热键的 `ExtraConfig` 会提取 `APIEndpoint`、`Token`、`TEXTPath` 作为运行时覆盖，并从 payload 中删除这三个字段。
- 全局 `ExtraConfig` 中同名字段只作为普通 payload 字段，不能解释为运行时覆盖。
- 单条运行时覆盖的字符串会去除首尾空白；空覆盖回退到全局配置值。
- 单条 `ExtraConfig` 非法时记录调试日志并忽略该单条扩展；全局 `ExtraConfig` 非法时启动失败。
- `messages` 固定包含两项：`developer` 内容为 `Prompt`，`user` 内容为选中文本。
- `Max_Tokens > 0` 时才加入 `max_tokens`；`temperature` 始终先加入，再允许扩展字段覆盖或删除。
- 递归删除 `null`、仅含空白的字符串、清理后为空的对象和数组；保留数字 `0` 与布尔值 `false`。

## 4. 响应提取

- `TEXTPath` 支持点分对象字段和每个 token 上的任意多个数组索引，如 `matrix[0][1].value`。
- 目标值是字符串、数字或布尔值时转为文本；对象、数组和 `null` 不可提取。
- 单条非空 `TEXTPath` 优先，否则使用配置中的 `TEXTPath`。
- 路径失败后先尝试顶层字符串字段 `text`，再返回任意一个非空顶层字符串。
- JSON 解析失败、顶层不是对象且路径失败、或没有可用值时返回空字符串。

## 5. HTTP、重试与取消

- 使用 JSON `POST`，请求头为 `Content-Type: application/json`、`User-Agent: clip-hotkey-client/1.0`，非空 token 使用 `Authorization: Bearer <token>`。
- 任意 `2xx` 状态成功；其他状态错误包含状态码与完整响应正文。
- `MaxRetry` 是总尝试次数，不是“首次请求之外的重试数”；小于等于 `0` 时仍尝试一次。
- 退避从 `RetryBaseDelay` 开始，每次乘 2；负值按 0 处理。请求和退避均可取消。
- 请求超时覆盖整个单次尝试，包括重定向和完整正文读取；非正超时不设置超时。
- 禁用环境和系统代理，以保持冻结的网络行为。
- 仅启用 gzip 自动解压，不默认启用 brotli 或 zstd。
- HTTP/2 开启时通过 TLS 协商；关闭时强制 HTTP/1.x。
- TLS 使用系统根证书；`VerifySSL=false` 时跳过证书验证。
- 默认在第 10 个重定向响应处停止（即最多发出 10 个请求）；`301`、`302`、`303` 将 `POST` 改为无正文 `GET`，`307`、`308` 保留方法和正文。
- 授权头仅发送到原域或其子域；一旦重定向到其他域，后续链路不再发送授权头。

## 6. 键盘与剪贴板

### `keybd_event`

`Ctrl+C` 和 `Ctrl+V` 完整复制 `micmonay/keybd_event v1.1.2` Windows 实现：

1. `Ctrl` 按下。
2. 主键按下。
3. `Ctrl` 释放。
4. 主键释放。

虚拟键偏移、扫描码判断、`bVk`、`bScan`、`KEYEVENTF_SCANCODE` 和 `KEYEVENTF_KEYUP` 的计算保持一致。不得改用 `SendInput` 或实现自动回退。

### Win32 文本剪贴板

- 仅处理 `CF_UNICODETEXT`。
- 使用 `OpenClipboard(0)`、`CloseClipboard`、`EmptyClipboard`、`GetClipboardData`、`SetClipboardData`、`GlobalAlloc(GMEM_MOVEABLE)`、`GlobalLock`、`GlobalUnlock`。
- 每次打开到关闭均在同一系统线程内完成。
- `OpenClipboard` 最多等待约 1 秒，每约 1 毫秒重试。
- 只备份和恢复文本格式。

复制时序：备份原文本；最多 5 次清空，每次失败等待 50 毫秒；无论最终是否清空成功都继续等待 50 毫秒并发送 `Ctrl+C`；每 50 毫秒轮询非空文本直到 `ClipboardTimeout`；最后等待 150 毫秒并最多 5 次恢复原文本，每次失败等待 50 毫秒。

粘贴时序：备份原文本；最多 5 次写入结果，每次失败等待 50 毫秒；成功后等待 `ClipboardWriteDelay`（默认 80 毫秒）；发送 `Ctrl+V`；等待 `ClipboardRestoreDelay`（默认 120 毫秒）后最多 5 次恢复原文本，每次失败等待 50 毫秒。CLI 参数 `--clipboard-write-delay` 和 `--clipboard-restore-delay` 可显式覆盖配置值，负值按 0 毫秒处理。

## 7. 热键

- 支持 `RegisterHotKey` 和 `WH_KEYBOARD_LL` 两种独立模式。
- 任务 ID 为 `HotKeyConfig` 的 1 基索引；停止热键内部 ID 为 `1000001`，只能产生停止事件。
- `RegisterHotKey` 不添加 `MOD_NOREPEAT`；退出时注销全部热键并结束专用消息线程。
- 低级钩子使用 `SetWindowsHookExW`、`GetAsyncKeyState` 和 `CallNextHookEx`；过滤 `LLKHF_INJECTED`；匹配时吞掉主键的按下与对应释放；回调内只做非阻塞事件分发。
- 不移植 STT 的长按防重复逻辑。
- 解析器支持 Ctrl/Control、Alt/Menu、Shift、Win/Meta/Super，字母、顶排数字、F1–F24、现有命名键、NumPad 数字及加减别名。
- 为保持既有配置兼容，位于主键前的未知修饰 token 会被忽略；非法主键在启动阶段报错。

## 8. 应用队列与停止语义

- 等待队列容量固定为 64，严格单工作者串行处理；队列满时非阻塞丢弃新任务。
- 执行顺序：复制选中文本、解析单条配置、构造请求、网络请求、提取响应、粘贴结果。
- 无有效选中文本时不发网络请求。
- 当前取消令牌只覆盖 HTTP 请求与退避，不中断剪贴板复制或粘贴。
- 停止热键取消当前 HTTP/退避并清空当时的等待队列，不关闭应用；停止后新入队任务仍可运行。
- 关闭应用时取消当前 HTTP/退避，清空队列并等待工作者结束。
- `RequestFailedNotification=true` 时，请求失败粘贴 `[request failed]`，提取为空粘贴 `[empty result]`；该字段不产生 Windows 通知。

## 9. 自动与手工验收

Linux 自动测试覆盖配置、CLI、请求、响应、HTTP、取消、剪贴板等待/重试以及队列语义。Ubuntu CI 运行格式检查、Linux 与 Windows 目标 Clippy、Rust 测试和 Windows GNU 交叉构建。

以下项目必须在 Windows 10 与 Windows 11 上手工完成，不能由 Linux 交叉编译代替：

- 记事本、浏览器文本框和常用编辑器中的复制、请求、替换和剪贴板恢复。
- `RegisterHotKey` 与 `WH_KEYBOARD_LL` 两种模式。
- 停止热键取消真实请求并在之后继续处理新任务。
- 注入的 `Ctrl+C`、`Ctrl+V` 被低级钩子识别为 injected 并正常放行。
- 退出后热键立即释放。
- 使用真实旧配置验证 `stp.exe` 无需迁移即可运行。
