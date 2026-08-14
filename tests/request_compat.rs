use serde_json::{Map, Value, json};
use stp::request::{
    BuildInput, build_payload, extract_runtime_overrides, merge_extra, parse_extra_config,
};

#[test]
fn request_merge_cleanup_and_precedence_match_contract() {
    let global = parse_extra_config(
        r#"{"model":"global","max_tokens":100,"verbosity":"low","nested":{"empty":"","keep":0},"arr":[null,"",false]}"#,
    )
    .unwrap();
    let entry = parse_extra_config(
        r#"{"APIEndpoint":" https://override ","Token":" token ","TEXTPath":" choices[0].text ","model":"","max_tokens":null,"verbosity":"","new_field":false,"zero":0}"#,
    )
    .unwrap();
    let (overrides, entry) = extract_runtime_overrides(entry);
    assert_eq!(overrides.api_endpoint, "https://override");
    assert_eq!(overrides.token, "token");
    assert_eq!(overrides.text_path, "choices[0].text");

    let payload = build_payload(BuildInput {
        model: "builtin",
        temperature: 0.0,
        max_tokens: 55,
        prompt: "translate",
        user_text: "hello",
        extra: merge_extra(global.as_ref(), entry.as_ref()),
    });
    assert_eq!(payload.get("model"), None);
    assert_eq!(payload.get("max_tokens"), None);
    assert_eq!(payload.get("verbosity"), None);
    assert_eq!(payload["nested"], json!({"keep": 0}));
    assert_eq!(payload["arr"], json!([false]));
    assert_eq!(payload["new_field"], false);
    assert_eq!(payload["zero"], 0);
}

#[test]
fn global_runtime_named_fields_are_payload_fields() {
    let global = parse_extra_config(
        r#"{"APIEndpoint":"payload-only","Token":"payload-token","TEXTPath":"payload-path"}"#,
    )
    .unwrap();
    let payload = build_payload(BuildInput {
        model: "",
        temperature: 0.0,
        max_tokens: 0,
        prompt: "p",
        user_text: "u",
        extra: global,
    });
    assert_eq!(payload["APIEndpoint"], "payload-only");
    assert_eq!(payload["Token"], "payload-token");
    assert_eq!(payload["TEXTPath"], "payload-path");
}

#[test]
fn payload_golden_is_semantically_stable() {
    let mut extra = Map::new();
    extra.insert("new_field".into(), Value::Bool(false));
    extra.insert("zero".into(), Value::from(0));
    let payload = build_payload(BuildInput {
        model: "",
        temperature: 0.0,
        max_tokens: 0,
        prompt: "translate",
        user_text: "hello",
        extra: Some(extra),
    });
    let golden: Value = serde_json::from_str(include_str!("golden/request-payload.json")).unwrap();
    assert_eq!(payload, golden);
}

#[test]
fn non_object_extra_config_remains_invalid() {
    assert!(parse_extra_config("[]").is_err());
    assert!(parse_extra_config("true").is_err());
    assert_eq!(parse_extra_config("null").unwrap(), None);
}
