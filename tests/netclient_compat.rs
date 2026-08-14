use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use stp::netclient::{
    Client, ClientOptions, NetError, NetworkClient, RetryOptions, USER_AGENT_VALUE,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

struct TestServer {
    endpoint: String,
    requests: Arc<AtomicUsize>,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn server(statuses: Vec<u16>, response_delay: Duration) -> TestServer {
    server_with_location(statuses, response_delay, None).await
}

async fn server_with_location(
    statuses: Vec<u16>,
    response_delay: Duration,
    location: Option<String>,
) -> TestServer {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let requests = Arc::new(AtomicUsize::new(0));
    let task_requests = requests.clone();
    let task = tokio::spawn(async move {
        loop {
            let (mut stream, _) = listener.accept().await.unwrap();
            let index = task_requests.fetch_add(1, Ordering::SeqCst);
            let status = statuses
                .get(index)
                .copied()
                .or_else(|| statuses.last().copied())
                .unwrap_or(200);
            let mut request = Vec::new();
            let mut buffer = [0u8; 1024];
            loop {
                let count = stream.read(&mut buffer).await.unwrap();
                if count == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..count]);
                if request.windows(4).any(|window| window == b"\r\n\r\n") {
                    break;
                }
            }
            assert!(
                String::from_utf8_lossy(&request)
                    .to_ascii_lowercase()
                    .contains(&format!("user-agent: {}", USER_AGENT_VALUE))
            );
            tokio::time::sleep(response_delay).await;
            let body = if (200..300).contains(&status) {
                br#"{"text":"ok"}"#.as_slice()
            } else {
                b"failure".as_slice()
            };
            let reason = if (200..300).contains(&status) {
                "OK"
            } else {
                "Error"
            };
            let location_header = location
                .as_deref()
                .map(|value| format!("Location: {value}\r\n"))
                .unwrap_or_default();
            let response = format!(
                "HTTP/1.1 {status} {reason}\r\n{location_header}Content-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
            stream.write_all(body).await.unwrap();
        }
    });
    TestServer {
        endpoint: format!("http://{address}"),
        requests,
        task,
    }
}

#[tokio::test]
async fn redirect_chain_uses_compatibility_request_limit() {
    let server = server_with_location(vec![307], Duration::ZERO, Some("/again".to_owned())).await;
    let error = client(5)
        .send_with_retry(
            CancellationToken::new(),
            &server.endpoint,
            "",
            &serde_json::json!({}),
            RetryOptions {
                max_retry: 1,
                base_delay_seconds: 0.0,
                debug: false,
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(error, NetError::TooManyRedirects(10)));
    assert_eq!(server.requests.load(Ordering::SeqCst), 10);
}

fn client(timeout: i64) -> Client {
    Client::new(ClientOptions {
        request_timeout_seconds: timeout,
        enable_http2: true,
        verify_ssl: true,
    })
    .unwrap()
}

#[tokio::test]
async fn retries_failures_then_accepts_any_2xx() {
    let server = server(vec![500, 502, 201], Duration::ZERO).await;
    let body = client(5)
        .send_with_retry(
            CancellationToken::new(),
            &server.endpoint,
            "token",
            &serde_json::json!({"x": 1}),
            RetryOptions {
                max_retry: 3,
                base_delay_seconds: 0.001,
                debug: false,
            },
        )
        .await
        .unwrap();
    assert_eq!(body, br#"{"text":"ok"}"#);
    assert_eq!(server.requests.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn max_retry_zero_still_attempts_once_and_non_2xx_keeps_body() {
    let server = server(vec![418], Duration::ZERO).await;
    let error = client(5)
        .send_with_retry(
            CancellationToken::new(),
            &server.endpoint,
            "",
            &serde_json::json!({}),
            RetryOptions {
                max_retry: 0,
                base_delay_seconds: 0.0,
                debug: false,
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(error, NetError::Status { status: 418, ref body } if body == "failure"));
    assert_eq!(server.requests.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn request_and_backoff_are_cancelable() {
    let slow = server(vec![200], Duration::from_secs(5)).await;
    let cancellation = CancellationToken::new();
    let task = {
        let cancellation = cancellation.clone();
        let endpoint = slow.endpoint.clone();
        tokio::spawn(async move {
            client(30)
                .send_with_retry(
                    cancellation,
                    &endpoint,
                    "",
                    &serde_json::json!({}),
                    RetryOptions {
                        max_retry: 1,
                        base_delay_seconds: 1.0,
                        debug: false,
                    },
                )
                .await
        })
    };
    while slow.requests.load(Ordering::SeqCst) == 0 {
        tokio::task::yield_now().await;
    }
    cancellation.cancel();
    assert!(matches!(task.await.unwrap(), Err(NetError::Cancelled)));

    let failing = server(vec![500, 200], Duration::ZERO).await;
    let cancellation = CancellationToken::new();
    let task = {
        let cancellation = cancellation.clone();
        let endpoint = failing.endpoint.clone();
        tokio::spawn(async move {
            client(30)
                .send_with_retry(
                    cancellation,
                    &endpoint,
                    "",
                    &serde_json::json!({}),
                    RetryOptions {
                        max_retry: 2,
                        base_delay_seconds: 30.0,
                        debug: false,
                    },
                )
                .await
        })
    };
    while failing.requests.load(Ordering::SeqCst) == 0 {
        tokio::task::yield_now().await;
    }
    tokio::time::sleep(Duration::from_millis(20)).await;
    cancellation.cancel();
    assert!(matches!(task.await.unwrap(), Err(NetError::Cancelled)));
    assert_eq!(failing.requests.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn configured_timeout_is_enforced() {
    let slow = server(vec![200], Duration::from_secs(3)).await;
    let error = client(1)
        .send_with_retry(
            CancellationToken::new(),
            &slow.endpoint,
            "",
            &serde_json::json!({}),
            RetryOptions {
                max_retry: 1,
                base_delay_seconds: 0.0,
                debug: false,
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(error, NetError::Timeout));
}
