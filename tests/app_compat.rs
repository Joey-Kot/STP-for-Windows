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
    Body(&'static [u8]),
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
            Mode::Body(body) => Ok(body.to_vec()),
        };
        self.active.fetch_sub(1, Ordering::SeqCst);
        result
    }
}

fn base_config() -> Config {
    Config {
        APIEndpoint: "https://example".to_owned(),
        MaxRetry: 1,
        TEXTPath: "$.text".to_owned(),
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
async fn request_failures_use_the_placeholder_when_enabled() {
    for (mode, placeholder) in [(Mode::Fail, "[request failed]")] {
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

#[test]
fn invalid_paths_are_rejected_before_copying_or_requesting() {
    let text_io = Arc::new(FakeTextIo::with_text("hello"));
    let network = Arc::new(FakeNetwork::new(Mode::Success));
    for path in ["", "text", "choices[0].message.content", "$.items["] {
        let mut config = base_config();
        config.TEXTPath = path.into();
        assert!(config.validate().is_err());
        assert!(App::new(config, network.clone(), text_io.clone()).is_err());
        if !path.is_empty() {
            let mut config = base_config();
            config.HotKeyConfig[0].ExtraConfig = serde_json::json!({"TEXTPath": path}).to_string();
            assert!(App::new(config, network.clone(), text_io.clone()).is_err());
        }
    }
    assert_eq!(text_io.copy_calls(), 0);
    assert_eq!(network.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn per_entry_path_precedes_global_and_blank_override_inherits() {
    for (path, expected) in [(" $.override ", "entry"), ("  ", "global")] {
        let mut config = base_config();
        config.HotKeyConfig[0].ExtraConfig = serde_json::json!({"TEXTPath": path}).to_string();
        let text_io = Arc::new(FakeTextIo::with_text("hello"));
        let network = Arc::new(FakeNetwork::new(Mode::Body(
            br#"{"text":"global","override":"entry"}"#,
        )));
        let app = App::new(config, network, text_io.clone()).unwrap();
        app.start().unwrap();
        app.enqueue_task(1);
        wait_for(|| text_io.pasted_contains(expected)).await;
        app.close().await.unwrap();
        assert_eq!(*text_io.pasted.lock().unwrap(), vec![expected]);
    }
}

struct ResponseSequence {
    first: &'static [u8],
    calls: AtomicUsize,
}

#[async_trait]
impl NetworkClient for ResponseSequence {
    async fn send_with_retry(
        &self,
        _: CancellationToken,
        _: &str,
        _: &str,
        _: &Value,
        _: RetryOptions,
    ) -> Result<Vec<u8>, NetError> {
        if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
            Ok(self.first.to_vec())
        } else {
            Ok(br#"{"text":"done","items":[{"text":"done"}]}"#.to_vec())
        }
    }
}

#[tokio::test]
async fn extraction_errors_and_empty_strings_do_not_retry_or_output_placeholders() {
    for (body, path) in [
        (b"not JSON".as_slice(), "$.text"),
        (br#"{"other":"fallback"}"#, "$.text"),
        (
            br#"{"text":"fallback","other":"fallback"}"#,
            "$.items[*].text",
        ),
        (br#"{"text":null}"#, "$.text"),
        (br#"{"text":{}}"#, "$.text"),
        (br#"{"text":[]}"#, "$.text"),
        (
            br#"{"items":[{"text":"a"},{"text":"b"}]}"#,
            "$.items[*].text",
        ),
        (br#"{"text":""}"#, "$.text"),
    ] {
        let mut config = base_config();
        config.TEXTPath = path.into();
        config.MaxRetry = 3;
        config.RequestFailedNotification = true;
        let text_io = Arc::new(FakeTextIo::with_text("hello"));
        let network = Arc::new(ResponseSequence {
            first: body,
            calls: AtomicUsize::new(0),
        });
        let app = App::new(config, network.clone(), text_io.clone()).unwrap();
        app.start().unwrap();
        app.enqueue_task(1);
        app.enqueue_task(1);
        // A second successful task is a completion barrier for the first one.
        wait_for(|| text_io.pasted_contains("done")).await;
        app.close().await.unwrap();
        assert_eq!(network.calls.load(Ordering::SeqCst), 2);
        assert_eq!(*text_io.pasted.lock().unwrap(), vec!["done"]);
    }
}

#[derive(Default)]
struct FakeOutput {
    sent: Mutex<Vec<String>>,
    started: Notify,
    wait_for_cancel: bool,
}

#[async_trait]
impl stp::text_input::TextOutput for FakeOutput {
    async fn send_text(
        &self,
        text: &str,
        cancellation: &CancellationToken,
    ) -> Result<(), stp::text_input::TextInputError> {
        assert!(!cancellation.is_cancelled());
        self.sent.lock().unwrap().push(text.into());
        self.started.notify_one();
        if self.wait_for_cancel {
            cancellation.cancelled().await;
            Err(stp::text_input::TextInputError::Canceled { sent: 256 })
        } else {
            Err(stp::text_input::TextInputError::Send {
                sent: 1,
                total: 4,
                code: 5,
            })
        }
    }
}

#[tokio::test]
async fn results_and_failure_placeholders_share_output_without_fallback_or_resend() {
    for (mode, expected) in [(Mode::Success, "ok"), (Mode::Fail, "[request failed]")] {
        let mut config = base_config();
        config.UseSendInput = true;
        config.RequestFailedNotification = true;
        let text_io = Arc::new(FakeTextIo::with_text("hello"));
        let output = Arc::new(FakeOutput::default());
        let app = App::with_output(
            config,
            Arc::new(FakeNetwork::new(mode)),
            text_io.clone(),
            output.clone(),
        )
        .unwrap();
        app.start().unwrap();
        app.enqueue_task(1);
        output.started.notified().await;
        app.close().await.unwrap();
        assert_eq!(*output.sent.lock().unwrap(), vec![expected]);
        assert!(text_io.pasted.lock().unwrap().is_empty());
    }
}

#[tokio::test]
async fn stop_and_close_cancel_output_and_stop_still_accepts_new_work() {
    let text_io = Arc::new(FakeTextIo::with_text("hello"));
    let output = Arc::new(FakeOutput {
        wait_for_cancel: true,
        ..Default::default()
    });
    let mut config = base_config();
    config.UseSendInput = true;
    config.RequestFailedNotification = true;
    let app = App::with_output(
        config,
        Arc::new(FakeNetwork::new(Mode::Success)),
        text_io.clone(),
        output.clone(),
    )
    .unwrap();
    app.start().unwrap();
    app.enqueue_task(1);
    output.started.notified().await;
    app.enqueue_task(1);
    app.stop_all();
    app.enqueue_task(1);
    tokio::time::timeout(Duration::from_secs(3), output.started.notified())
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(3), app.close())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(text_io.copy_calls(), 2);
    assert_eq!(*output.sent.lock().unwrap(), vec!["ok", "ok"]);
    assert!(text_io.pasted.lock().unwrap().is_empty());
}
