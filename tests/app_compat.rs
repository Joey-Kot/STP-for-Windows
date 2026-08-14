use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;
use stp::app::App;
use stp::clipboard::{TextIo, TextIoError};
use stp::config::{Config, HotKeyEntry};
use stp::netclient::{NetError, NetworkClient, RetryOptions};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

#[derive(Default)]
struct FakeTextIo {
    copy_text: String,
    copy_calls: AtomicUsize,
    pasted: Mutex<Vec<String>>,
}

impl FakeTextIo {
    fn with_text(text: &str) -> Self {
        Self {
            copy_text: text.to_owned(),
            ..Self::default()
        }
    }

    fn copy_calls(&self) -> usize {
        self.copy_calls.load(Ordering::SeqCst)
    }

    fn pasted_contains(&self, expected: &str) -> bool {
        self.pasted
            .lock()
            .unwrap()
            .iter()
            .any(|text| text == expected)
    }
}

impl TextIo for FakeTextIo {
    fn copy_selected(&self) -> Result<String, TextIoError> {
        self.copy_calls.fetch_add(1, Ordering::SeqCst);
        Ok(self.copy_text.clone())
    }

    fn paste_text(&self, text: &str) -> Result<(), TextIoError> {
        self.pasted.lock().unwrap().push(text.to_owned());
        Ok(())
    }
}

#[derive(Clone, Copy)]
enum Mode {
    Success,
    Fail,
    Empty,
}

struct RecoveringNetwork {
    started: Notify,
    calls: AtomicUsize,
}

impl RecoveringNetwork {
    fn new() -> Self {
        Self {
            started: Notify::new(),
            calls: AtomicUsize::new(0),
        }
    }
}

#[async_trait]
impl NetworkClient for RecoveringNetwork {
    async fn send_with_retry(
        &self,
        cancellation: CancellationToken,
        _endpoint: &str,
        _token: &str,
        _payload: &Value,
        _options: RetryOptions,
    ) -> Result<Vec<u8>, NetError> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        if call == 0 {
            self.started.notify_one();
            cancellation.cancelled().await;
            Err(NetError::Cancelled)
        } else {
            Ok(br#"{"text":"ok"}"#.to_vec())
        }
    }
}

struct FakeNetwork {
    mode: Mode,
    started: Notify,
    active: AtomicUsize,
    max_active: AtomicUsize,
    calls: AtomicUsize,
}

impl FakeNetwork {
    fn new(mode: Mode) -> Self {
        Self {
            mode,
            started: Notify::new(),
            active: AtomicUsize::new(0),
            max_active: AtomicUsize::new(0),
            calls: AtomicUsize::new(0),
        }
    }
}

#[async_trait]
impl NetworkClient for FakeNetwork {
    async fn send_with_retry(
        &self,
        _cancellation: CancellationToken,
        _endpoint: &str,
        _token: &str,
        _payload: &Value,
        _options: RetryOptions,
    ) -> Result<Vec<u8>, NetError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.max_active.fetch_max(active, Ordering::SeqCst);
        self.started.notify_one();
        let result = match self.mode {
            Mode::Success => {
                tokio::time::sleep(Duration::from_millis(3)).await;
                Ok(br#"{"text":"ok"}"#.to_vec())
            }
            Mode::Fail => Err(NetError::Failed),
            Mode::Empty => Ok(b"{}".to_vec()),
        };
        self.active.fetch_sub(1, Ordering::SeqCst);
        result
    }
}

fn base_config() -> Config {
    Config {
        APIEndpoint: "https://example".to_owned(),
        MaxRetry: 1,
        HotKeyConfig: vec![HotKeyEntry {
            Prompt: "translate".to_owned(),
            HotKey: "ctrl+f1".to_owned(),
            ExtraConfig: String::new(),
        }],
        ..Config::default()
    }
}

async fn wait_for(check: impl Fn() -> bool) {
    tokio::time::timeout(Duration::from_secs(3), async {
        while !check() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("condition timed out");
}

#[tokio::test]
async fn placeholders_match_request_and_empty_result_behavior() {
    for (mode, placeholder) in [
        (Mode::Fail, "[request failed]"),
        (Mode::Empty, "[empty result]"),
    ] {
        let mut config = base_config();
        config.RequestFailedNotification = true;
        let text_io = Arc::new(FakeTextIo::with_text("hello"));
        let app = App::new(config, Arc::new(FakeNetwork::new(mode)), text_io.clone()).unwrap();
        app.start().unwrap();
        app.enqueue_task(1);
        wait_for(|| text_io.pasted_contains(placeholder)).await;
        app.close().await.unwrap();
    }
}

#[tokio::test]
async fn twenty_concurrent_enqueues_execute_strictly_serially() {
    let text_io = Arc::new(FakeTextIo::with_text("hello"));
    let network = Arc::new(FakeNetwork::new(Mode::Success));
    let app = App::new(base_config(), network.clone(), text_io.clone()).unwrap();
    app.start().unwrap();

    let mut joins = Vec::new();
    for _ in 0..20 {
        let app = app.clone();
        joins.push(tokio::spawn(async move { app.enqueue_task(1) }));
    }
    for join in joins {
        join.await.unwrap();
    }
    wait_for(|| text_io.copy_calls() == 20).await;
    assert_eq!(network.max_active.load(Ordering::SeqCst), 1);
    app.close().await.unwrap();
}

#[tokio::test]
async fn stop_cancels_current_clears_old_queue_and_accepts_new_work() {
    let text_io = Arc::new(FakeTextIo::with_text("hello"));
    let network = Arc::new(RecoveringNetwork::new());
    let app = App::new(base_config(), network.clone(), text_io.clone()).unwrap();
    app.start().unwrap();
    app.enqueue_task(1);
    network.started.notified().await;
    app.enqueue_task(1);
    app.stop_all();

    tokio::time::sleep(Duration::from_millis(80)).await;
    assert_eq!(text_io.copy_calls(), 1);

    // The same controller remains alive. A newly enqueued task starts after
    // the canceled request unwinds, proving StopAll is not a permanent stop.
    app.enqueue_task(1);
    wait_for(|| text_io.pasted_contains("ok")).await;
    assert_eq!(text_io.copy_calls(), 2);
    app.close().await.unwrap();
}

#[tokio::test]
async fn blank_selected_text_skips_network_request() {
    let text_io = Arc::new(FakeTextIo::with_text("   "));
    let network = Arc::new(FakeNetwork::new(Mode::Success));
    let app = App::new(base_config(), network.clone(), text_io.clone()).unwrap();
    app.start().unwrap();
    app.enqueue_task(1);
    wait_for(|| text_io.copy_calls() == 1).await;
    tokio::time::sleep(Duration::from_millis(30)).await;
    assert_eq!(network.calls.load(Ordering::SeqCst), 0);
    app.close().await.unwrap();
}
