// Copyright (C) 2026 Joey Kot <joey.kot.x@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later

use serde_json::{Map, Value, json};
use thiserror::Error;

pub type ExtraConfig = Map<String, Value>;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RuntimeOverrides {
    pub api_endpoint: String,
    pub token: String,
    pub text_path: String,
}

#[derive(Debug, Error)]
pub enum ExtraConfigError {
    #[error("invalid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("ExtraConfig must be a JSON object or null")]
    NotObject,
}

pub fn parse_extra_config(raw: &str) -> Result<Option<ExtraConfig>, ExtraConfigError> {
    if raw.trim().is_empty() {
        return Ok(None);
    }
    match serde_json::from_str::<Value>(raw)? {
        Value::Null => Ok(None),
        Value::Object(map) => Ok(Some(map)),
        _ => Err(ExtraConfigError::NotObject),
    }
}

pub fn extract_runtime_overrides(
    extra: Option<ExtraConfig>,
) -> (RuntimeOverrides, Option<ExtraConfig>) {
    let Some(mut clean) = extra else {
        return (RuntimeOverrides::default(), None);
    };

    let mut overrides = RuntimeOverrides::default();
    if let Some(value) = clean.remove("APIEndpoint")
        && let Value::String(value) = value
    {
        overrides.api_endpoint = value.trim().to_owned();
    }
    if let Some(value) = clean.remove("Token")
        && let Value::String(value) = value
    {
        overrides.token = value.trim().to_owned();
    }
    if let Some(value) = clean.remove("TEXTPath")
        && let Value::String(value) = value
    {
        overrides.text_path = value.trim().to_owned();
    }

    let clean = (!clean.is_empty()).then_some(clean);
    (overrides, clean)
}

pub fn merge_extra(
    base: Option<&ExtraConfig>,
    override_values: Option<&ExtraConfig>,
) -> Option<ExtraConfig> {
    let mut output = ExtraConfig::new();
    if let Some(base) = base {
        output.extend(base.iter().map(|(key, value)| (key.clone(), value.clone())));
    }
    if let Some(override_values) = override_values {
        output.extend(
            override_values
                .iter()
                .map(|(key, value)| (key.clone(), value.clone())),
        );
    }
    (!output.is_empty()).then_some(output)
}

pub fn strip_empty_fields(map: ExtraConfig) -> ExtraConfig {
    map.into_iter()
        .filter_map(|(key, value)| clean_value(value).map(|value| (key, value)))
        .collect()
}

fn clean_value(value: Value) -> Option<Value> {
    match value {
        Value::Null => None,
        Value::String(value) if value.trim().is_empty() => None,
        Value::String(value) => Some(Value::String(value)),
        Value::Object(map) => {
            let map = strip_empty_fields(map);
            (!map.is_empty()).then_some(Value::Object(map))
        }
        Value::Array(values) => {
            let values: Vec<_> = values.into_iter().filter_map(clean_value).collect();
            (!values.is_empty()).then_some(Value::Array(values))
        }
        other => Some(other),
    }
}

pub struct BuildInput<'a> {
    pub model: &'a str,
    pub temperature: f64,
    pub max_tokens: i64,
    pub prompt: &'a str,
    pub user_text: &'a str,
    pub extra: Option<ExtraConfig>,
}

pub fn build_payload(input: BuildInput<'_>) -> Value {
    let mut payload = ExtraConfig::new();
    if !input.model.is_empty() {
        payload.insert("model".to_owned(), Value::String(input.model.to_owned()));
    }
    payload.insert(
        "messages".to_owned(),
        json!([
            {"role": "developer", "content": input.prompt},
            {"role": "user", "content": input.user_text}
        ]),
    );
    if input.max_tokens > 0 {
        payload.insert("max_tokens".to_owned(), Value::from(input.max_tokens));
    }
    payload.insert("temperature".to_owned(), json!(input.temperature));
    if let Some(extra) = input.extra {
        payload.extend(extra);
    }
    Value::Object(strip_empty_fields(payload))
}
