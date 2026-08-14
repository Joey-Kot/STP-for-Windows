// Copyright (C) 2026 Joey Kot <joey.kot.x@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later

pub mod parse;
mod windows;

use std::collections::BTreeMap;
use std::sync::Arc;

pub use windows::SystemHotkeyService;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    Task(i32),
    Stop,
}

#[derive(Clone, Debug, Default)]
pub struct Options {
    pub use_hook: bool,
    pub task_hotkeys: BTreeMap<i32, String>,
    pub stop_task_hotkey: String,
}

pub type Handler = Arc<dyn Fn(Event) + Send + Sync + 'static>;

pub trait Service {
    fn start(&self, handler: Handler) -> Result<(), HotkeyError>;
    fn close(&self) -> Result<(), HotkeyError>;
}

#[derive(Debug, thiserror::Error)]
pub enum HotkeyError {
    #[error("invalid hotkey '{spec}' for id={id}: {source}")]
    InvalidTask {
        spec: String,
        id: i32,
        #[source]
        source: parse::ParseHotkeyError,
    },
    #[error("invalid stop hotkey '{spec}': {source}")]
    InvalidStop {
        spec: String,
        #[source]
        source: parse::ParseHotkeyError,
    },
    #[error("hotkey service is only supported on Windows")]
    Unsupported,
    #[error("hotkey service is already running")]
    AlreadyRunning,
    #[error("hotkey service startup timed out")]
    StartupTimeout,
    #[error("{0}")]
    Platform(String),
}
