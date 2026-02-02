package config

import (
	"encoding/json"
	"os"
)

type HotKeyEntry struct {
	Prompt      string `json:"Prompt"`
	HotKey      string `json:"HotKey"`
	ExtraConfig string `json:"ExtraConfig"`
}

type Config struct {
	APIEndpoint               string        `json:"APIEndpoint"`
	Token                     string        `json:"Token"`
	Model                     string        `json:"Model"`
	Temperature               float64       `json:"Temperature"`
	MaxTokens                 int           `json:"Max_Tokens"`
	TEXTPath                  string        `json:"TEXTPath"`
	ExtraConfig               string        `json:"ExtraConfig"`
	RequestTimeout            int           `json:"RequestTimeout"`
	MaxRetry                  int           `json:"MaxRetry"`
	RetryBaseDelay            float64       `json:"RetryBaseDelay"`
	EnableHTTP2               bool          `json:"EnableHTTP2"`
	VerifySSL                 bool          `json:"VerifySSL"`
	ClipboardTimeout          int           `json:"ClipboardTimeout"`
	RequestFailedNotification bool          `json:"RequestFailedNotification"`
	StopTaskHotkey            string        `json:"StopTaskHotkey"`
	HotKeyConfig              []HotKeyEntry `json:"HotKeyConfig"`
	HotKeyHook                bool          `json:"HotKeyHook"`
	DEBUG                     bool          `json:"DEBUG"`
}

func Default() Config {
	return Config{
		APIEndpoint:               "",
		Token:                     "",
		Model:                     "",
		Temperature:               0.0,
		MaxTokens:                 0,
		TEXTPath:                  "choices[0].message.content",
		ExtraConfig:               "",
		RequestTimeout:            30,
		MaxRetry:                  3,
		RetryBaseDelay:            0.5,
		EnableHTTP2:               true,
		VerifySSL:                 true,
		ClipboardTimeout:          1000,
		RequestFailedNotification: false,
		StopTaskHotkey:            "",
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
		HotKeyHook: false,
		DEBUG:      false,
	}
}

func Load(path string) (Config, error) {
	cfg := Default()
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

func SaveDefault(path string) error {
	b, err := json.MarshalIndent(Default(), "", "  ")
	if err != nil {
		return err
	}
	return os.WriteFile(path, b, 0o644)
}
