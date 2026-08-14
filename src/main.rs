// Copyright (C) 2026 Joey Kot <joey.kot.x@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::BTreeMap;
use std::env;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use stp::app::App;
use stp::clipboard::{Manager, SystemClipboard};
use stp::config::{Cli, ConfigSelection};
use stp::hotkey::{Event, Options, Service, SystemHotkeyService};
use stp::keyboard::SystemKeySimulator;
use stp::netclient::{Client, ClientOptions};

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(code) => ExitCode::from(code),
    }
}

async fn run() -> Result<(), u8> {
    let cli = match Cli::try_parse_args(env::args_os()) {
        Ok(cli) => cli,
        Err(error) => {
            let code = stp::config::clap_exit_code(&error);
            let _ = error.print();
            return if code == 0 { Ok(()) } else { Err(2) };
        }
    };
    let cwd = env::current_dir().map_err(|error| {
        eprintln!("[main] failed to determine current directory: {error}");
        1
    })?;
    let mut config = match stp::config::select(&cli, &cwd) {
        Ok(ConfigSelection::Loaded(config)) => *config,
        Ok(ConfigSelection::DefaultCreated(path)) => {
            println!(
                "[main] default {} created. Please edit it and re-run.",
                path.file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("config.json")
            );
            return Ok(());
        }
        Err(error) => {
            eprintln!("[main] {error}");
            return Err(1);
        }
    };
    cli.apply_to(&mut config);

    let network = Arc::new(
        Client::new(ClientOptions {
            request_timeout_seconds: config.RequestTimeout,
            enable_http2: config.EnableHTTP2,
            verify_ssl: config.VerifySSL,
        })
        .map_err(|error| {
            eprintln!("[main] {error}");
            1
        })?,
    );
    let timeout = Duration::from_millis(config.ClipboardTimeout.max(0) as u64);
    let write_delay = Duration::from_millis(config.ClipboardWriteDelay.max(0) as u64);
    let restore_delay = Duration::from_millis(config.ClipboardRestoreDelay.max(0) as u64);
    let text_io = Arc::new(Manager::new(
        Arc::new(SystemClipboard::new()),
        Arc::new(SystemKeySimulator::new()),
        timeout,
        write_delay,
        restore_delay,
    ));
    let application = App::new(config.clone(), network, text_io).map_err(|error| {
        eprintln!("[main] {error}");
        1
    })?;
    application.start().map_err(|error| {
        eprintln!("[main] {error}");
        1
    })?;

    let task_hotkeys: BTreeMap<_, _> = config
        .HotKeyConfig
        .iter()
        .enumerate()
        .filter(|(_, entry)| !entry.Prompt.trim().is_empty() && !entry.HotKey.trim().is_empty())
        .map(|(index, entry)| ((index + 1) as i32, entry.HotKey.clone()))
        .collect();
    if task_hotkeys.is_empty() {
        println!("[main] no prompts configured; nothing to register. Exiting.");
        let _ = application.close().await;
        return Ok(());
    }

    let hotkeys = SystemHotkeyService::new(Options {
        use_hook: config.HotKeyHook,
        task_hotkeys,
        stop_task_hotkey: config.StopTaskHotkey.clone(),
    });
    let handler_app = Arc::clone(&application);
    hotkeys
        .start(Arc::new(move |event| match event {
            Event::Task(id) => handler_app.enqueue_task(id),
            Event::Stop => handler_app.stop_all(),
        }))
        .map_err(|error| {
            eprintln!("[main] failed to start hotkey service: {error}");
            1
        })?;

    println!("[main] ready. Press configured hotkeys to invoke. Ctrl+C to exit.");
    if let Err(error) = tokio::signal::ctrl_c().await {
        eprintln!("[main] failed to wait for Ctrl+C: {error}");
    }
    println!("[main] exiting");

    if let Err(error) = hotkeys.close() {
        eprintln!("[main] failed to close hotkey service: {error}");
    }
    if let Err(error) = application.close().await {
        eprintln!("[main] failed to close application: {error}");
        return Err(1);
    }
    Ok(())
}
