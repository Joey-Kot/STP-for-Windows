# STP-for-Windows
这是一个面向 Windows 平台的基于本地剪贴板与全局热键的 LLM 文本处理客户端 (Select Text Process)。按下配置的热键后，程序会复制当前选中的文本（模拟 Ctrl+C），将文本和对应的提示词（Prompt）一起发送到配置的 API 端点，然后将返回的文本粘贴回当前焦点（模拟 Ctrl+V）。适用于需要通过快捷键快速调用远程/本地 LLM 处理选中文本的场景。
