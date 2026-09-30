// Copyright (C) 2026 Joey Kot <joey.kot.x@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later

use serde_json::Value;
use serde_json_path::{JsonPath, ParseError};
use thiserror::Error;

#[derive(Debug, Error)]
#[error("invalid TEXTPath: {0}")]
pub struct TextPathError(#[from] ParseError);

#[derive(Debug, Error)]
pub enum TextExtractionError {
    #[error("API response is not valid JSON: {0}")]
    InvalidJson(#[from] serde_json::Error),
    #[error("TEXTPath matched no values; expected exactly one")]
    NoMatch,
    #[error("TEXTPath matched {count} values; expected exactly one")]
    MultipleMatches { count: usize },
    #[error("TEXTPath selected {kind}; expected a string, number or boolean")]
    InvalidType { kind: &'static str },
}

pub fn parse_text_path(path: &str) -> Result<JsonPath, TextPathError> {
    Ok(JsonPath::parse(path)?)
}

pub fn extract_text_from_response(
    body: &[u8],
    text_path: &JsonPath,
) -> Result<String, TextExtractionError> {
    let root: Value = serde_json::from_slice(body)?;
    let nodes = text_path.query(&root);
    let count = nodes.len();
    let value = nodes.exactly_one().map_err(|_| {
        if count == 0 {
            TextExtractionError::NoMatch
        } else {
            TextExtractionError::MultipleMatches { count }
        }
    })?;
    match value {
        Value::String(text) => Ok(text.clone()),
        Value::Number(number) => Ok(number.to_string()),
        Value::Bool(value) => Ok(value.to_string()),
        Value::Null => Err(TextExtractionError::InvalidType { kind: "null" }),
        Value::Array(_) => Err(TextExtractionError::InvalidType { kind: "an array" }),
        Value::Object(_) => Err(TextExtractionError::InvalidType { kind: "an object" }),
    }
}
