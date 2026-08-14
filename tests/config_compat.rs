use std::fs;

use clap::CommandFactory;
use stp::config::{Cli, Config, ConfigSelection, select};

#[test]
fn default_config_matches_golden_data() {
    let expected: serde_json::Value =
        serde_json::from_str(include_str!("golden/default-config.json")).unwrap();
    assert_eq!(serde_json::to_value(Config::default()).unwrap(), expected);
}

#[test]
fn missing_fields_keep_defaults_and_unknown_fields_are_ignored() {
    let config: Config = serde_json::from_str(
        r#"{"APIEndpoint":"https://example","Unknown":"ignored","HotKeyConfig":[{"Prompt":"x"}]}"#,
    )
    .unwrap();
    assert_eq!(config.APIEndpoint, "https://example");
    assert_eq!(config.RequestTimeout, 30);
    assert_eq!(config.ClipboardWriteDelay, 80);
    assert_eq!(config.ClipboardRestoreDelay, 120);
    assert!(config.EnableHTTP2);
    assert_eq!(config.HotKeyConfig.len(), 1);
    assert_eq!(config.HotKeyConfig[0].Prompt, "x");
    assert_eq!(config.HotKeyConfig[0].HotKey, "");
}

#[test]
fn json_null_preserves_legacy_unmarshal_behavior() {
    let config: Config = serde_json::from_str(
        r#"{"APIEndpoint":null,"RequestTimeout":null,"EnableHTTP2":null,"ClipboardWriteDelay":null,"ClipboardRestoreDelay":null,"HotKeyConfig":null}"#,
    )
    .unwrap();
    assert_eq!(config.APIEndpoint, "");
    assert_eq!(config.RequestTimeout, 30);
    assert_eq!(config.ClipboardWriteDelay, 80);
    assert_eq!(config.ClipboardRestoreDelay, 120);
    assert!(config.EnableHTTP2);
    assert!(config.HotKeyConfig.is_empty());

    let config: Config = serde_json::from_str("null").unwrap();
    assert_eq!(config, Config::default());

    let config: Config =
        serde_json::from_str(r#"{"HotKeyConfig":[null,{"Prompt":null}]}"#).unwrap();
    assert_eq!(config.HotKeyConfig.len(), 2);
    assert_eq!(config.HotKeyConfig[0], Default::default());
    assert_eq!(config.HotKeyConfig[1], Default::default());
}

#[test]
fn clipboard_delay_cli_values_override_only_when_present() {
    let cli = Cli::try_parse_args([
        "stp",
        "--clipboard-write-delay",
        "95",
        "--clipboard-restore-delay=135",
    ])
    .unwrap();
    let mut config = Config::default();
    cli.apply_to(&mut config);
    assert_eq!(config.ClipboardWriteDelay, 95);
    assert_eq!(config.ClipboardRestoreDelay, 135);

    let absent = Cli::try_parse_args(["stp", "--model", "x"]).unwrap();
    let mut preserved = Config {
        ClipboardWriteDelay: 12,
        ClipboardRestoreDelay: 34,
        ..Config::default()
    };
    absent.apply_to(&mut preserved);
    assert_eq!(preserved.ClipboardWriteDelay, 12);
    assert_eq!(preserved.ClipboardRestoreDelay, 34);
}

#[test]
fn help_uses_grouped_sister_cli_layout() {
    let help = Cli::command().render_long_help().to_string();
    for expected in [
        "Process selected text through an LLM API and paste the result.",
        "Usage: stp.exe [OPTIONS]",
        "Options:",
        "General:",
        "API:",
        "Network:",
        "Hotkeys:",
        "Output:",
        "Debug:",
        "-V, --version",
        "--clipboard-write-delay <MS>",
        "--clipboard-restore-delay <MS>",
    ] {
        assert!(help.contains(expected), "missing help text: {expected}");
    }
}

#[test]
fn bool_cli_values_override_only_when_present() {
    let cli = Cli::try_parse_args([
        "stp",
        "--verify-ssl",
        "false",
        "--enable-http2=true",
        "--hotkey-hook",
        "false",
        "--debug=yes",
    ])
    .unwrap();
    let mut config = Config {
        VerifySSL: true,
        EnableHTTP2: false,
        HotKeyHook: true,
        DEBUG: false,
        ..Config::default()
    };
    cli.apply_to(&mut config);
    assert!(!config.VerifySSL);
    assert!(config.EnableHTTP2);
    assert!(!config.HotKeyHook);
    assert!(config.DEBUG);

    let absent = Cli::try_parse_args(["stp", "--model", "x"]).unwrap();
    let mut preserved = Config {
        VerifySSL: false,
        EnableHTTP2: false,
        HotKeyHook: true,
        ..Config::default()
    };
    absent.apply_to(&mut preserved);
    assert!(!preserved.VerifySSL);
    assert!(!preserved.EnableHTTP2);
    assert!(preserved.HotKeyHook);
}

#[test]
fn config_selection_matches_compatibility_rules() {
    let directory = tempfile::tempdir().unwrap();
    let no_overrides = Cli::try_parse_args(["stp"]).unwrap();
    match select(&no_overrides, directory.path()).unwrap() {
        ConfigSelection::DefaultCreated(path) => {
            assert_eq!(path, directory.path().join("config.json"));
            assert!(path.is_file());
        }
        ConfigSelection::Loaded(_) => panic!("expected default creation"),
    }

    let saved: serde_json::Value =
        serde_json::from_slice(&fs::read(directory.path().join("config.json")).unwrap()).unwrap();
    assert_eq!(
        saved,
        serde_json::from_str::<serde_json::Value>(include_str!("golden/default-config.json"))
            .unwrap()
    );

    let other = tempfile::tempdir().unwrap();
    let override_cli = Cli::try_parse_args(["stp", "--model", "override"]).unwrap();
    match select(&override_cli, other.path()).unwrap() {
        ConfigSelection::Loaded(config) => assert_eq!(*config, Config::default()),
        ConfigSelection::DefaultCreated(_) => panic!("override must start from defaults"),
    }
    assert!(!other.path().join("config.json").exists());
}

#[test]
fn empty_explicit_config_path_starts_from_defaults() {
    let directory = tempfile::tempdir().unwrap();
    let cli = Cli {
        config: Some(Default::default()),
        api_endpoint: None,
        token: None,
        model: None,
        temperature: None,
        max_tokens: None,
        text_path: None,
        extra_config: None,
        request_timeout: None,
        max_retry: None,
        retry_base_delay: None,
        enable_http2: None,
        verify_ssl: None,
        clipboard_timeout: None,
        clipboard_write_delay: None,
        clipboard_restore_delay: None,
        request_failed_notification: None,
        stop_task_hotkey: None,
        hotkey_hook: None,
        debug: None,
    };
    match select(&cli, directory.path()).unwrap() {
        ConfigSelection::Loaded(config) => assert_eq!(*config, Config::default()),
        ConfigSelection::DefaultCreated(_) => panic!("empty explicit path must not create a file"),
    }
    assert!(!directory.path().join("config.json").exists());
}

#[test]
fn old_single_dash_long_options_are_rejected() {
    let error = Cli::try_parse_args(["stp", "-api-endpoint", "https://example"]).unwrap_err();
    assert_ne!(stp::config::clap_exit_code(&error), 0);
}

#[test]
fn empty_string_override_is_rejected_as_missing_value() {
    assert!(Cli::try_parse_args(["stp", "--model="]).is_err());
    assert!(Cli::try_parse_args(["stp", "--stop-task-hotkey="]).is_err());
}
