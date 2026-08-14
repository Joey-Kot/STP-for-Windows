// Copyright (C) 2026 Joey Kot <joey.kot.x@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later

use std::time::Duration;

use async_trait::async_trait;
use reqwest::header::{CONTENT_TYPE, HeaderValue, LOCATION, REFERER, USER_AGENT};
use reqwest::redirect::Policy;
use reqwest::{Method, StatusCode, Url};
use serde_json::Value;
use thiserror::Error;
use tokio_util::sync::CancellationToken;

pub const USER_AGENT_VALUE: &str = "clip-hotkey-client/1.0";
const MAX_REDIRECTS: usize = 10;

#[derive(Clone, Copy, Debug)]
pub struct ClientOptions {
    pub request_timeout_seconds: i64,
    pub enable_http2: bool,
    pub verify_ssl: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct RetryOptions {
    pub max_retry: i64,
    pub base_delay_seconds: f64,
    pub debug: bool,
}

#[derive(Debug, Error)]
pub enum NetError {
    #[error("API endpoint empty")]
    EmptyEndpoint,
    #[error("invalid API endpoint: {0}")]
    InvalidEndpoint(#[from] url::ParseError),
    #[error("failed to build HTTP client: {0}")]
    BuildClient(reqwest::Error),
    #[error("invalid Authorization header: {0}")]
    InvalidAuthorizationHeader(#[from] reqwest::header::InvalidHeaderValue),
    #[error("failed to serialize request JSON: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error("request canceled")]
    Cancelled,
    #[error("request timed out")]
    Timeout,
    #[error("request failed: {0}")]
    Request(reqwest::Error),
    #[error("invalid redirect Location header")]
    InvalidRedirectLocation,
    #[error("stopped after {0} consecutive redirect requests")]
    TooManyRedirects(usize),
    #[error("status {status}: {body}")]
    Status { status: u16, body: String },
    #[error("request failed")]
    Failed,
}

#[async_trait]
pub trait NetworkClient: Send + Sync {
    async fn send_with_retry(
        &self,
        cancellation: CancellationToken,
        endpoint: &str,
        token: &str,
        payload: &Value,
        options: RetryOptions,
    ) -> Result<Vec<u8>, NetError>;
}

pub struct Client {
    inner: reqwest::Client,
    request_timeout: Option<Duration>,
}

impl Client {
    pub fn new(options: ClientOptions) -> Result<Self, NetError> {
        let mut builder = reqwest::Client::builder()
            // The frozen compatibility contract disables environment and
            // system proxy discovery.
            .no_proxy()
            // net/http follows at most ten redirects by default.
            // Redirects are implemented below so the frozen method,
            // body-header and Authorization propagation rules remain explicit.
            .redirect(Policy::none())
            .referer(false)
            .gzip(true)
            .danger_accept_invalid_certs(!options.verify_ssl)
            .pool_idle_timeout(Duration::from_secs(90))
            .pool_max_idle_per_host(100);

        if !options.enable_http2 {
            builder = builder.http1_only();
        }
        let inner = builder.build().map_err(NetError::BuildClient)?;
        let request_timeout = (options.request_timeout_seconds > 0)
            .then(|| Duration::from_secs(options.request_timeout_seconds as u64));
        Ok(Self {
            inner,
            request_timeout,
        })
    }

    async fn one_attempt(
        &self,
        cancellation: &CancellationToken,
        endpoint: Url,
        token: &str,
        data: Vec<u8>,
    ) -> Result<Vec<u8>, NetError> {
        let future = self.follow_redirects(cancellation, endpoint, token, data);
        if let Some(timeout) = self.request_timeout {
            match tokio::time::timeout(timeout, future).await {
                Ok(result) => result,
                Err(_) => Err(NetError::Timeout),
            }
        } else {
            future.await
        }
    }

    async fn follow_redirects(
        &self,
        cancellation: &CancellationToken,
        endpoint: Url,
        token: &str,
        data: Vec<u8>,
    ) -> Result<Vec<u8>, NetError> {
        let initial_url = endpoint.clone();
        let mut current_url = endpoint;
        let mut previous_url: Option<Url> = None;
        let mut method = Method::POST;
        let mut include_body = true;
        let mut strip_authorization = false;
        let mut redirect_count = 0usize;

        loop {
            let mut builder = self
                .inner
                .request(method.clone(), current_url.clone())
                .header(USER_AGENT, HeaderValue::from_static(USER_AGENT_VALUE));
            if include_body {
                builder = builder
                    .header(CONTENT_TYPE, HeaderValue::from_static("application/json"))
                    .body(data.clone());
            }
            if !token.is_empty() && !strip_authorization {
                builder = builder.bearer_auth(token);
            }
            if let Some(previous_url) = &previous_url
                && let Some(referer) = referer_for_url(previous_url, &current_url)
            {
                builder = builder.header(REFERER, referer);
            }
            let request = builder.build().map_err(NetError::Request)?;
            let response = tokio::select! {
                biased;
                _ = cancellation.cancelled() => return Err(NetError::Cancelled),
                response = self.inner.execute(request) => response.map_err(NetError::Request)?,
            };
            let status = response.status();

            if is_redirect_status(status)
                && let Some(location) = response.headers().get(LOCATION)
            {
                redirect_count += 1;
                if redirect_count >= MAX_REDIRECTS {
                    return Err(NetError::TooManyRedirects(redirect_count));
                }
                let location = location
                    .to_str()
                    .map_err(|_| NetError::InvalidRedirectLocation)?;
                let next_url = current_url
                    .join(location)
                    .map_err(|_| NetError::InvalidRedirectLocation)?;
                if !strip_authorization && !should_copy_authorization(&initial_url, &next_url) {
                    strip_authorization = true;
                }

                if matches!(
                    status,
                    StatusCode::MOVED_PERMANENTLY | StatusCode::FOUND | StatusCode::SEE_OTHER
                ) {
                    method = Method::GET;
                    include_body = false;
                }
                // Read the redirect body before dropping the response. A full
                // read is deterministic and preserves reuse without exposing
                // the body.
                let _ = tokio::select! {
                    biased;
                    _ = cancellation.cancelled() => return Err(NetError::Cancelled),
                    body = response.bytes() => body.map_err(NetError::Request)?,
                };
                previous_url = Some(current_url);
                current_url = next_url;
                continue;
            }

            let body = tokio::select! {
                biased;
                _ = cancellation.cancelled() => return Err(NetError::Cancelled),
                body = response.bytes() => body.map_err(NetError::Request)?.to_vec(),
            };
            if status.is_success() {
                return Ok(body);
            }
            return Err(NetError::Status {
                status: status.as_u16(),
                body: String::from_utf8_lossy(&body).into_owned(),
            });
        }
    }
}

fn is_redirect_status(status: StatusCode) -> bool {
    matches!(
        status,
        StatusCode::MOVED_PERMANENTLY
            | StatusCode::FOUND
            | StatusCode::SEE_OTHER
            | StatusCode::TEMPORARY_REDIRECT
            | StatusCode::PERMANENT_REDIRECT
    )
}

fn should_copy_authorization(initial: &Url, destination: &Url) -> bool {
    let Some(initial_host) = initial.host_str() else {
        return false;
    };
    let Some(destination_host) = destination.host_str() else {
        return false;
    };
    if destination_host == initial_host {
        return true;
    }
    if destination_host.contains([':', '%']) || !destination_host.ends_with(initial_host) {
        return false;
    }
    destination_host.as_bytes().get(
        destination_host
            .len()
            .saturating_sub(initial_host.len() + 1),
    ) == Some(&b'.')
}

fn referer_for_url(previous: &Url, destination: &Url) -> Option<HeaderValue> {
    if previous.scheme() == "https" && destination.scheme() == "http" {
        return None;
    }
    let mut referer = previous.clone();
    let _ = referer.set_username("");
    let _ = referer.set_password(None);
    referer.set_fragment(None);
    HeaderValue::from_str(referer.as_str()).ok()
}

#[async_trait]
impl NetworkClient for Client {
    async fn send_with_retry(
        &self,
        cancellation: CancellationToken,
        endpoint: &str,
        token: &str,
        payload: &Value,
        options: RetryOptions,
    ) -> Result<Vec<u8>, NetError> {
        if endpoint.is_empty() {
            return Err(NetError::EmptyEndpoint);
        }
        let endpoint = reqwest::Url::parse(endpoint)?;
        let data = serde_json::to_vec(payload)?;
        let attempts = options.max_retry.max(1);
        let mut delay = if !options.base_delay_seconds.is_finite()
            || options.base_delay_seconds.is_sign_negative()
        {
            Duration::ZERO
        } else {
            Duration::from_secs_f64(options.base_delay_seconds)
        };
        let mut last_error = None;

        for attempt in 1..=attempts {
            match self
                .one_attempt(&cancellation, endpoint.clone(), token, data.clone())
                .await
            {
                Ok(body) => return Ok(body),
                Err(NetError::Cancelled) => return Err(NetError::Cancelled),
                Err(error) => last_error = Some(error),
            }

            if attempt == attempts {
                break;
            }
            tokio::select! {
                biased;
                _ = cancellation.cancelled() => return Err(NetError::Cancelled),
                _ = tokio::time::sleep(delay) => {}
            }
            delay = delay.saturating_mul(2);
        }
        Err(last_error.unwrap_or(NetError::Failed))
    }
}
