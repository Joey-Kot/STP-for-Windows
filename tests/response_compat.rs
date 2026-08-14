use stp::response::extract_text_from_response;

#[test]
fn extracts_strings_numbers_booleans_and_nested_array_indexes() {
    let body = br#"{"choices":[{"message":{"content":"hello"}}],"matrix":[[{"value":false}]],"integer":12,"float":12.5}"#;
    assert_eq!(
        extract_text_from_response(body, "choices[0].message.content", ""),
        "hello"
    );
    assert_eq!(
        extract_text_from_response(body, "matrix[0][0].value", ""),
        "false"
    );
    assert_eq!(extract_text_from_response(body, "integer", ""), "12");
    assert_eq!(extract_text_from_response(body, "float", ""), "12.5");
}

#[test]
fn override_path_precedes_default_and_blank_override_uses_default() {
    let body = br#"{"override":"x","default":"y"}"#;
    assert_eq!(extract_text_from_response(body, "override", "default"), "x");
    assert_eq!(extract_text_from_response(body, "  ", "default"), "y");
}

#[test]
fn failed_path_uses_text_then_another_top_level_string() {
    assert_eq!(
        extract_text_from_response(br#"{"other":"x","text":"fallback"}"#, "bad.path", ""),
        "fallback"
    );
    assert_eq!(
        extract_text_from_response(br#"{"nested":{},"other":"x"}"#, "bad.path", ""),
        "x"
    );
    assert_eq!(extract_text_from_response(b"not json", "text", ""), "");
}
