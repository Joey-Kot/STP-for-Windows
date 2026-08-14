// Copyright (C) 2026 Joey Kot <joey.kot.x@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later

use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use clap::builder::BoolishValueParser;
use clap::builder::NonEmptyStringValueParser;
use clap::{Parser, error::ErrorKind};
use serde::{Deserialize, Deserializer, Serialize};
use thiserror::Error;

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[allow(non_snake_case)]
pub struct HotKeyEntry {
    pub Prompt: String,
    pub HotKey: String,
    pub ExtraConfig: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[allow(non_snake_case)]
pub struct Config {
    pub APIEndpoint: String,
    pub Token: String,
    pub Model: String,
    pub Temperature: f64,
    #[serde(rename = "Max_Tokens")]
    pub MaxTokens: i64,
    pub TEXTPath: String,
    pub ExtraConfig: String,
    pub RequestTimeout: i64,
    pub MaxRetry: i64,
    pub RetryBaseDelay: f64,
    pub EnableHTTP2: bool,
    pub VerifySSL: bool,
    pub ClipboardTimeout: i64,
    pub ClipboardWriteDelay: i64,
    pub ClipboardRestoreDelay: i64,
    pub RequestFailedNotification: bool,
    pub StopTaskHotkey: String,
    pub HotKeyConfig: Vec<HotKeyEntry>,
    pub HotKeyHook: bool,
    pub DEBUG: bool,
}

#[derive(Default, Deserialize)]
#[allow(non_snake_case)]
struct RawHotKeyEntry {
    Prompt: Option<String>,
    HotKey: Option<String>,
    ExtraConfig: Option<String>,
}

impl From<Option<RawHotKeyEntry>> for HotKeyEntry {
    fn from(raw: Option<RawHotKeyEntry>) -> Self {
        let raw = raw.unwrap_or_default();
        Self {
            Prompt: raw.Prompt.unwrap_or_default(),
            HotKey: raw.HotKey.unwrap_or_default(),
            ExtraConfig: raw.ExtraConfig.unwrap_or_default(),
        }
    }
}

impl<'de> Deserialize<'de> for HotKeyEntry {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Option::<RawHotKeyEntry>::deserialize(deserializer).map(Into::into)
    }
}

#[derive(Default)]
enum Nullable<T> {
    #[default]
    Missing,
    Null,
    Value(T),
}

impl<'de, T> Deserialize<'de> for Nullable<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Option::<T>::deserialize(deserializer).map(|value| match value {
            Some(value) => Self::Value(value),
            None => Self::Null,
        })
    }
}

#[derive(Default, Deserialize)]
#[allow(non_snake_case)]
struct RawConfig {
    APIEndpoint: Option<String>,
    Token: Option<String>,
    Model: Option<String>,
    Temperature: Option<f64>,
    #[serde(rename = "Max_Tokens")]
    MaxTokens: Option<i64>,
    TEXTPath: Option<String>,
    ExtraConfig: Option<String>,
    RequestTimeout: Option<i64>,
    MaxRetry: Option<i64>,
    RetryBaseDelay: Option<f64>,
    EnableHTTP2: Option<bool>,
    VerifySSL: Option<bool>,
    ClipboardTimeout: Option<i64>,
    ClipboardWriteDelay: Option<i64>,
    ClipboardRestoreDelay: Option<i64>,
    RequestFailedNotification: Option<bool>,
    StopTaskHotkey: Option<String>,
    #[serde(default)]
    HotKeyConfig: Nullable<Vec<Option<RawHotKeyEntry>>>,
    HotKeyHook: Option<bool>,
    DEBUG: Option<bool>,
}

impl From<Option<RawConfig>> for Config {
    fn from(raw: Option<RawConfig>) -> Self {
        let Some(raw) = raw else {
            return Self::default();
        };
        let mut config = Self::default();
        if let Some(value) = raw.APIEndpoint {
            config.APIEndpoint = value;
        }
        if let Some(value) = raw.Token {
            config.Token = value;
        }
        if let Some(value) = raw.Model {
            config.Model = value;
        }
        if let Some(value) = raw.Temperature {
            config.Temperature = value;
        }
        if let Some(value) = raw.MaxTokens {
            config.MaxTokens = value;
        }
        if let Some(value) = raw.TEXTPath {
            config.TEXTPath = value;
        }
        if let Some(value) = raw.ExtraConfig {
            config.ExtraConfig = value;
        }
        if let Some(value) = raw.RequestTimeout {
            config.RequestTimeout = value;
        }
        if let Some(value) = raw.MaxRetry {
            config.MaxRetry = value;
        }
        if let Some(value) = raw.RetryBaseDelay {
            config.RetryBaseDelay = value;
        }
        if let Some(value) = raw.EnableHTTP2 {
            config.EnableHTTP2 = value;
        }
        if let Some(value) = raw.VerifySSL {
            config.VerifySSL = value;
        }
        if let Some(value) = raw.ClipboardTimeout {
            config.ClipboardTimeout = value;
        }
        if let Some(value) = raw.ClipboardWriteDelay {
            config.ClipboardWriteDelay = value;
        }
        if let Some(value) = raw.ClipboardRestoreDelay {
            config.ClipboardRestoreDelay = value;
        }
        if let Some(value) = raw.RequestFailedNotification {
            config.RequestFailedNotification = value;
        }
        if let Some(value) = raw.StopTaskHotkey {
            config.StopTaskHotkey = value;
        }
        match raw.HotKeyConfig {
            Nullable::Missing => {}
            Nullable::Null => config.HotKeyConfig.clear(),
            Nullable::Value(entries) => {
                config.HotKeyConfig = entries.into_iter().map(Into::into).collect();
            }
        }
        if let Some(value) = raw.HotKeyHook {
            config.HotKeyHook = value;
        }
        if let Some(value) = raw.DEBUG {
            config.DEBUG = value;
        }
        config
    }
}

impl<'de> Deserialize<'de> for Config {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Option::<RawConfig>::deserialize(deserializer).map(Into::into)
    }
}

impl Default for Config {
    fn default() -> Self {
        let mut hotkeys = Vec::with_capacity(10);
        for index in 1..=8 {
            hotkeys.push(HotKeyEntry {
                Prompt: String::new(),
                HotKey: format!("ctrl+f{index}"),
                ExtraConfig: String::new(),
            });
        }
        hotkeys.push(HotKeyEntry::default());
        hotkeys.push(HotKeyEntry::default());

        Self {
            APIEndpoint: String::new(),
            Token: String::new(),
            Model: String::new(),
            Temperature: 0.0,
            MaxTokens: 0,
            TEXTPath: "choices[0].message.content".to_owned(),
            ExtraConfig: String::new(),
            RequestTimeout: 30,
            MaxRetry: 3,
            RetryBaseDelay: 0.5,
            EnableHTTP2: true,
            VerifySSL: true,
            ClipboardTimeout: 1000,
            ClipboardWriteDelay: 80,
            ClipboardRestoreDelay: 120,
            RequestFailedNotification: false,
            StopTaskHotkey: String::new(),
            HotKeyConfig: hotkeys,
            HotKeyHook: false,
            DEBUG: false,
        }
    }
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read config '{path}': {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("invalid config JSON in '{path}': {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("failed to serialize default config: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error("failed to write default config '{path}': {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to inspect config '{path}': {source}")]
    Inspect {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

pub fn load(path: impl AsRef<Path>) -> Result<Config, ConfigError> {
    let path = path.as_ref();
    let data = fs::read(path).map_err(|source| ConfigError::Read {
        path: path.to_owned(),
        source,
    })?;
    serde_json::from_slice(&data).map_err(|source| ConfigError::Parse {
        path: path.to_owned(),
        source,
    })
}

pub fn save_default(path: impl AsRef<Path>) -> Result<(), ConfigError> {
    let path = path.as_ref();
    let mut data = serde_json::to_vec_pretty(&Config::default())?;
    data.push(b'\n');
    fs::write(path, data).map_err(|source| ConfigError::Write {
        path: path.to_owned(),
        source,
    })
}

#[derive(Clone, Debug, Parser, PartialEq)]
#[command(
    name = "stp.exe",
    version,
    about = "Process selected text through an LLM API and paste the result.",
    long_about = None
)]
pub struct Cli {
    /// Path to a JSON configuration file
    #[arg(long, value_name = "PATH", help_heading = "General")]
    pub config: Option<PathBuf>,

    /// LLM HTTP endpoint URL
    #[arg(
        long,
        value_name = "URL",
        help_heading = "API",
        value_parser = NonEmptyStringValueParser::new()
    )]
    pub api_endpoint: Option<String>,
    /// Bearer token sent with the LLM request
    #[arg(
        long,
        value_name = "TOKEN",
        help_heading = "API",
        value_parser = NonEmptyStringValueParser::new()
    )]
    pub token: Option<String>,
    /// Model request field
    #[arg(
        long,
        value_name = "MODEL",
        help_heading = "API",
        value_parser = NonEmptyStringValueParser::new()
    )]
    pub model: Option<String>,
    /// Temperature request field
    #[arg(long, value_name = "VALUE", help_heading = "API")]
    pub temperature: Option<f64>,
    /// Maximum token request field; omitted when not positive
    #[arg(long, value_name = "N", help_heading = "API")]
    pub max_tokens: Option<i64>,
    /// Dot path used to extract text from the JSON response
    #[arg(
        long,
        value_name = "PATH",
        help_heading = "API",
        value_parser = NonEmptyStringValueParser::new()
    )]
    pub text_path: Option<String>,
    /// Stringified JSON object with extra request fields
    #[arg(
        long,
        value_name = "JSON",
        help_heading = "API",
        value_parser = NonEmptyStringValueParser::new()
    )]
    pub extra_config: Option<String>,
    /// Per-request client timeout in seconds
    #[arg(long, value_name = "SECONDS", help_heading = "Network")]
    pub request_timeout: Option<i64>,
    /// Maximum request attempts, including the first request
    #[arg(long, value_name = "N", help_heading = "Network")]
    pub max_retry: Option<i64>,
    /// Initial exponential-backoff delay in seconds
    #[arg(long, value_name = "SECONDS", help_heading = "Network")]
    pub retry_base_delay: Option<f64>,
    /// Enable HTTP/2 negotiation
    #[arg(
        long,
        value_name = "BOOL",
        help_heading = "Network",
        value_parser = BoolishValueParser::new(),
        num_args = 1
    )]
    pub enable_http2: Option<bool>,
    /// Verify TLS certificates
    #[arg(
        long,
        value_name = "BOOL",
        help_heading = "Network",
        value_parser = BoolishValueParser::new(),
        num_args = 1
    )]
    pub verify_ssl: Option<bool>,
    /// Cancel the current request and clear queued tasks
    #[arg(
        long,
        value_name = "HOTKEY",
        help_heading = "Hotkeys",
        value_parser = NonEmptyStringValueParser::new()
    )]
    pub stop_task_hotkey: Option<String>,
    /// Use the low-level keyboard hook instead of RegisterHotKey
    #[arg(
        long,
        value_name = "BOOL",
        help_heading = "Hotkeys",
        value_parser = BoolishValueParser::new(),
        num_args = 1
    )]
    pub hotkey_hook: Option<bool>,
    /// Maximum milliseconds to wait for copied selection text
    #[arg(long, value_name = "MS", help_heading = "Hotkeys")]
    pub clipboard_timeout: Option<i64>,
    /// Milliseconds to wait after writing the processed text before sending Ctrl+V
    #[arg(long, value_name = "MS", help_heading = "Hotkeys")]
    pub clipboard_write_delay: Option<i64>,
    /// Milliseconds to wait after Ctrl+V before restoring the original clipboard text
    #[arg(long, value_name = "MS", help_heading = "Hotkeys")]
    pub clipboard_restore_delay: Option<i64>,
    /// Paste failure or empty-result placeholders when enabled
    #[arg(
        long,
        value_name = "BOOL",
        help_heading = "Output",
        value_parser = BoolishValueParser::new(),
        num_args = 1
    )]
    pub request_failed_notification: Option<bool>,
    /// Enable diagnostic logging
    #[arg(
        long,
        value_name = "BOOL",
        help_heading = "Debug",
        value_parser = BoolishValueParser::new(),
        num_args = 1
    )]
    pub debug: Option<bool>,
}

impl Cli {
    pub fn try_parse_args<I, T>(args: I) -> Result<Self, clap::Error>
    where
        I: IntoIterator<Item = T>,
        T: Into<OsString> + Clone,
    {
        Self::try_parse_from(args)
    }

    pub fn has_overrides(&self) -> bool {
        self.api_endpoint.is_some()
            || self.token.is_some()
            || self.model.is_some()
            || self.temperature.is_some()
            || self.max_tokens.is_some()
            || self.text_path.is_some()
            || self.extra_config.is_some()
            || self.request_timeout.is_some()
            || self.max_retry.is_some()
            || self.retry_base_delay.is_some()
            || self.enable_http2.is_some()
            || self.verify_ssl.is_some()
            || self.clipboard_timeout.is_some()
            || self.clipboard_write_delay.is_some()
            || self.clipboard_restore_delay.is_some()
            || self.request_failed_notification.is_some()
            || self.stop_task_hotkey.is_some()
            || self.hotkey_hook.is_some()
            || self.debug.is_some()
    }

    pub fn apply_to(&self, config: &mut Config) {
        if let Some(value) = &self.api_endpoint {
            config.APIEndpoint.clone_from(value);
        }
        if let Some(value) = &self.token {
            config.Token.clone_from(value);
        }
        if let Some(value) = &self.model {
            config.Model.clone_from(value);
        }
        if let Some(value) = self.temperature {
            config.Temperature = value;
        }
        if let Some(value) = self.max_tokens {
            config.MaxTokens = value;
        }
        if let Some(value) = &self.text_path {
            config.TEXTPath.clone_from(value);
        }
        if let Some(value) = &self.extra_config {
            config.ExtraConfig.clone_from(value);
        }
        if let Some(value) = self.request_timeout {
            config.RequestTimeout = value;
        }
        if let Some(value) = self.max_retry {
            config.MaxRetry = value;
        }
        if let Some(value) = self.retry_base_delay {
            config.RetryBaseDelay = value;
        }
        if let Some(value) = self.enable_http2 {
            config.EnableHTTP2 = value;
        }
        if let Some(value) = self.verify_ssl {
            config.VerifySSL = value;
        }
        if let Some(value) = self.clipboard_timeout {
            config.ClipboardTimeout = value;
        }
        if let Some(value) = self.clipboard_write_delay {
            config.ClipboardWriteDelay = value;
        }
        if let Some(value) = self.clipboard_restore_delay {
            config.ClipboardRestoreDelay = value;
        }
        if let Some(value) = self.request_failed_notification {
            config.RequestFailedNotification = value;
        }
        if let Some(value) = &self.stop_task_hotkey {
            config.StopTaskHotkey.clone_from(value);
        }
        if let Some(value) = self.hotkey_hook {
            config.HotKeyHook = value;
        }
        if let Some(value) = self.debug {
            config.DEBUG = value;
        }
    }
}

pub enum ConfigSelection {
    Loaded(Box<Config>),
    DefaultCreated(PathBuf),
}

pub fn select(cli: &Cli, cwd: impl AsRef<Path>) -> Result<ConfigSelection, ConfigError> {
    if let Some(path) = &cli.config {
        if path.as_os_str().is_empty() {
            return Ok(ConfigSelection::Loaded(Box::default()));
        }
        return load(path).map(|config| ConfigSelection::Loaded(Box::new(config)));
    }

    let default_path = cwd.as_ref().join("config.json");
    match fs::metadata(&default_path) {
        Ok(_) => load(&default_path).map(|config| ConfigSelection::Loaded(Box::new(config))),
        Err(source) if source.kind() == io::ErrorKind::NotFound => {
            if cli.has_overrides() {
                Ok(ConfigSelection::Loaded(Box::default()))
            } else {
                save_default(&default_path)?;
                Ok(ConfigSelection::DefaultCreated(default_path))
            }
        }
        Err(source) => Err(ConfigError::Inspect {
            path: default_path,
            source,
        }),
    }
}

pub fn clap_exit_code(error: &clap::Error) -> i32 {
    match error.kind() {
        ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => 0,
        _ => 2,
    }
}
