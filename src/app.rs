// Copyright (C) 2026 Joey Kot <joey.kot.x@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use thiserror::Error;
use tokio::sync::Notify;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::clipboard::TextIo;
use crate::config::Config;
use crate::netclient::{NetworkClient, RetryOptions};
use crate::request::{
    BuildInput, ExtraConfig, build_payload, extract_runtime_overrides, merge_extra,
    parse_extra_config,
};
use crate::response::extract_text_from_response;

pub const TASK_QUEUE_CAPACITY: usize = 64;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("invalid ExtraConfig JSON: {0}")]
    InvalidGlobalExtra(#[from] crate::request::ExtraConfigError),
    #[error("application has already been started")]
    AlreadyStarted,
    #[error("application worker panicked: {0}")]
    Worker(#[from] tokio::task::JoinError),
}

struct State {
    queue: VecDeque<i32>,
    current_cancellation: Option<CancellationToken>,
    closed: bool,
}

pub struct App {
    config: Config,
    network: Arc<dyn NetworkClient>,
    text_io: Arc<dyn TextIo>,
    global_extra: Option<ExtraConfig>,
    state: Mutex<State>,
    notify: Notify,
    started: AtomicBool,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl App {
    pub fn new(
        config: Config,
        network: Arc<dyn NetworkClient>,
        text_io: Arc<dyn TextIo>,
    ) -> Result<Arc<Self>, AppError> {
        let global_extra = parse_extra_config(&config.ExtraConfig)?;
        Ok(Arc::new(Self {
            config,
            network,
            text_io,
            global_extra,
            state: Mutex::new(State {
                queue: VecDeque::with_capacity(TASK_QUEUE_CAPACITY),
                current_cancellation: None,
                closed: false,
            }),
            notify: Notify::new(),
            started: AtomicBool::new(false),
            worker: Mutex::new(None),
        }))
    }

    pub fn start(self: &Arc<Self>) -> Result<(), AppError> {
        if self.started.swap(true, Ordering::AcqRel) {
            return Err(AppError::AlreadyStarted);
        }
        let app = Arc::clone(self);
        let handle = tokio::spawn(async move { app.run().await });
        *self.worker.lock().expect("app worker state poisoned") = Some(handle);
        Ok(())
    }

    pub fn enqueue_task(&self, id: i32) {
        let mut state = self.state.lock().expect("app state poisoned");
        if state.closed {
            return;
        }
        if state.queue.len() == TASK_QUEUE_CAPACITY {
            if self.config.DEBUG {
                eprintln!("[app] queue full, dropped task id={id}");
            }
            return;
        }
        state.queue.push_back(id);
        drop(state);
        self.notify.notify_one();
    }

    pub fn stop_all(&self) {
        let mut state = self.state.lock().expect("app state poisoned");
        if let Some(cancellation) = &state.current_cancellation {
            cancellation.cancel();
        }
        state.queue.clear();
    }

    pub async fn close(&self) -> Result<(), AppError> {
        {
            let mut state = self.state.lock().expect("app state poisoned");
            if !state.closed {
                state.closed = true;
                state.queue.clear();
                if let Some(cancellation) = &state.current_cancellation {
                    cancellation.cancel();
                }
            }
        }
        self.notify.notify_waiters();
        let handle = self
            .worker
            .lock()
            .expect("app worker state poisoned")
            .take();
        if let Some(handle) = handle {
            handle.await?;
        }
        Ok(())
    }

    async fn run(self: Arc<Self>) {
        loop {
            let notified = self.notify.notified();
            let task = {
                let mut state = self.state.lock().expect("app state poisoned");
                if state.closed {
                    return;
                }
                state.queue.pop_front()
            };
            if let Some(id) = task {
                self.handle_task(id).await;
            } else {
                notified.await;
            }
        }
    }

    async fn handle_task(&self, id: i32) {
        if id < 1 {
            return;
        }
        let Some(entry) = self.config.HotKeyConfig.get((id - 1) as usize) else {
            return;
        };
        let prompt = entry.Prompt.trim().to_owned();
        if prompt.is_empty() {
            return;
        }

        let selected_text = match self.copy_selected().await {
            Ok(text) if !text.trim().is_empty() => text,
            Ok(_) => return,
            Err(error) => {
                if self.config.DEBUG {
                    eprintln!("[copy] failed: {error}");
                }
                return;
            }
        };

        let per_entry_extra = match parse_extra_config(&entry.ExtraConfig) {
            Ok(value) => value,
            Err(error) => {
                if self.config.DEBUG {
                    eprintln!("[request] invalid entry ExtraConfig id={id}: {error}");
                }
                None
            }
        };
        let (runtime_overrides, per_entry_payload) = extract_runtime_overrides(per_entry_extra);
        let payload = build_payload(BuildInput {
            model: &self.config.Model,
            temperature: self.config.Temperature,
            max_tokens: self.config.MaxTokens,
            prompt: &prompt,
            user_text: &selected_text,
            extra: merge_extra(self.global_extra.as_ref(), per_entry_payload.as_ref()),
        });

        let endpoint = if runtime_overrides.api_endpoint.trim().is_empty() {
            self.config.APIEndpoint.trim()
        } else {
            runtime_overrides.api_endpoint.trim()
        };
        let token = if runtime_overrides.token.trim().is_empty() {
            self.config.Token.trim()
        } else {
            runtime_overrides.token.trim()
        };
        let cancellation = CancellationToken::new();
        {
            let mut state = self.state.lock().expect("app state poisoned");
            state.current_cancellation = Some(cancellation.clone());
        }
        let response = self
            .network
            .send_with_retry(
                cancellation.clone(),
                endpoint,
                token,
                &payload,
                RetryOptions {
                    max_retry: self.config.MaxRetry,
                    base_delay_seconds: self.config.RetryBaseDelay,
                    debug: self.config.DEBUG,
                },
            )
            .await;
        self.clear_current(&cancellation);

        let body = match response {
            Ok(body) => body,
            Err(error) => {
                if self.config.DEBUG {
                    eprintln!("[request] failed: {error}");
                }
                self.notify_placeholder("[request failed]").await;
                return;
            }
        };
        let extracted =
            extract_text_from_response(&body, &runtime_overrides.text_path, &self.config.TEXTPath);
        if extracted.trim().is_empty() {
            self.notify_placeholder("[empty result]").await;
            return;
        }
        if let Err(error) = self.paste_text(extracted).await
            && self.config.DEBUG
        {
            eprintln!("[paste] failed: {error}");
        }
    }

    fn clear_current(&self, expected: &CancellationToken) {
        let mut state = self.state.lock().expect("app state poisoned");
        if state
            .current_cancellation
            .as_ref()
            .is_some_and(|current| current == expected)
        {
            state.current_cancellation = None;
        }
    }

    async fn copy_selected(&self) -> Result<String, String> {
        let text_io = Arc::clone(&self.text_io);
        tokio::task::spawn_blocking(move || text_io.copy_selected())
            .await
            .map_err(|error| error.to_string())?
            .map_err(|error| error.to_string())
    }

    async fn paste_text(&self, text: String) -> Result<(), String> {
        let text_io = Arc::clone(&self.text_io);
        tokio::task::spawn_blocking(move || text_io.paste_text(&text))
            .await
            .map_err(|error| error.to_string())?
            .map_err(|error| error.to_string())
    }

    async fn notify_placeholder(&self, text: &str) {
        if self.config.RequestFailedNotification {
            let _ = self.paste_text(text.to_owned()).await;
        }
    }
}
