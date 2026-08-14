// Copyright (C) 2026 Joey Kot <joey.kot.x@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later

use serde_json::Value;

pub fn extract_text_from_response(
    body: &[u8],
    override_text_path: &str,
    default_text_path: &str,
) -> String {
    let Ok(root) = serde_json::from_slice::<Value>(body) else {
        return String::new();
    };

    let text_path = if override_text_path.trim().is_empty() {
        default_text_path.trim()
    } else {
        override_text_path.trim()
    };
    if !text_path.is_empty()
        && let Some(output) = extract_by_path(&root, text_path)
    {
        return output;
    }

    let Value::Object(map) = root else {
        return String::new();
    };
    if let Some(Value::String(text)) = map.get("text") {
        return text.clone();
    }
    map.values()
        .find_map(|value| match value {
            Value::String(text) if !text.is_empty() => Some(text.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

fn extract_by_path(root: &Value, path: &str) -> Option<String> {
    if path.is_empty() {
        return None;
    }
    let mut current = root;
    for token in path.split('.') {
        let (key, indexes) = parse_key_and_indexes(token)?;
        if !key.is_empty() {
            current = current.as_object()?.get(key)?;
        }
        for index in indexes {
            current = current.as_array()?.get(index)?;
        }
    }
    match current {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

fn parse_key_and_indexes(token: &str) -> Option<(&str, Vec<usize>)> {
    if token.is_empty() {
        return None;
    }
    let Some(first_bracket) = token.find('[') else {
        return Some((token, Vec::new()));
    };
    let key = &token[..first_bracket];
    let mut rest = &token[first_bracket..];
    let mut indexes = Vec::new();
    while !rest.is_empty() {
        if !rest.starts_with('[') {
            return None;
        }
        let closing = rest.find(']')?;
        let index = rest[1..closing].parse::<usize>().ok()?;
        indexes.push(index);
        rest = &rest[closing + 1..];
    }
    Some((key, indexes))
}

#[cfg(test)]
mod tests {
    use super::extract_text_from_response;

    #[test]
    fn supports_multiple_indexes_in_one_token() {
        let body = br#"{"matrix":[[true, 12.5]]}"#;
        assert_eq!(extract_text_from_response(body, "matrix[0][0]", ""), "true");
        assert_eq!(extract_text_from_response(body, "matrix[0][1]", ""), "12.5");
    }
}
