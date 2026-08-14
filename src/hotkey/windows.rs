// Copyright (C) 2026 Joey Kot <joey.kot.x@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later

#[cfg(windows)]
mod implementation {
    use std::collections::{HashMap, HashSet};
    use std::io;
    use std::sync::mpsc::{self, SyncSender, TrySendError};
    use std::sync::{Arc, Mutex};
    use std::thread::{self, JoinHandle};
    use std::time::Duration;

    use super::super::parse::{
        MOD_ALT, MOD_CONTROL, MOD_SHIFT, MOD_WIN, ParsedHotkey, parse_hotkey,
    };
    use super::super::{Event, Handler, HotkeyError, Options, Service};

    const WH_KEYBOARD_LL: i32 = 13;
    const LLKHF_INJECTED: u32 = 0x0000_0010;
    const WM_KEYDOWN: u32 = 0x0100;
    const WM_KEYUP: u32 = 0x0101;
    const WM_SYSKEYDOWN: u32 = 0x0104;
    const WM_SYSKEYUP: u32 = 0x0105;
    const WM_HOTKEY: u32 = 0x0312;
    const WM_QUIT: u32 = 0x0012;
    const VK_CONTROL: i32 = 0x11;
    const VK_MENU: i32 = 0x12;
    const VK_SHIFT: i32 = 0x10;
    const VK_LWIN: i32 = 0x5b;
    const VK_RWIN: i32 = 0x5c;
    const STOP_HOTKEY_ID: i32 = 1_000_001;

    #[derive(Clone, Copy)]
    struct TaskSpec {
        id: i32,
        parsed: ParsedHotkey,
    }

    struct Running {
        windows_thread_id: u32,
        windows_thread: JoinHandle<()>,
        event_tx: SyncSender<Event>,
        dispatcher_thread: JoinHandle<()>,
    }

    pub struct SystemHotkeyService {
        options: Options,
        running: Mutex<Option<Running>>,
    }

    impl SystemHotkeyService {
        pub fn new(options: Options) -> Self {
            Self {
                options,
                running: Mutex::new(None),
            }
        }

        fn build_entries(&self) -> Result<Vec<TaskSpec>, HotkeyError> {
            let mut entries = Vec::new();
            for (&id, spec) in &self.options.task_hotkeys {
                if spec.trim().is_empty() {
                    continue;
                }
                let parsed = parse_hotkey(spec).map_err(|source| HotkeyError::InvalidTask {
                    spec: spec.clone(),
                    id,
                    source,
                })?;
                entries.push(TaskSpec { id, parsed });
            }
            if !self.options.stop_task_hotkey.trim().is_empty() {
                let parsed = parse_hotkey(&self.options.stop_task_hotkey).map_err(|source| {
                    HotkeyError::InvalidStop {
                        spec: self.options.stop_task_hotkey.clone(),
                        source,
                    }
                })?;
                entries.push(TaskSpec {
                    id: STOP_HOTKEY_ID,
                    parsed,
                });
            }
            Ok(entries)
        }
    }

    impl Service for SystemHotkeyService {
        fn start(&self, handler: Handler) -> Result<(), HotkeyError> {
            let entries = self.build_entries()?;
            if entries.is_empty() {
                return Ok(());
            }

            let mut running = self.running.lock().expect("hotkey state poisoned");
            if running.is_some() {
                return Err(HotkeyError::AlreadyRunning);
            }

            let (event_tx, event_rx) = mpsc::sync_channel::<Event>(32);
            let dispatcher_thread = thread::spawn(move || {
                while let Ok(event) = event_rx.recv() {
                    handler(event);
                }
            });

            let (startup_tx, startup_rx) = mpsc::sync_channel(1);
            let (thread_id_tx, thread_id_rx) = mpsc::sync_channel(1);
            let thread_event_tx = event_tx.clone();
            let use_hook = self.options.use_hook;
            let windows_thread = thread::spawn(move || {
                let thread_id = unsafe { GetCurrentThreadId() };
                let mut message = Message::default();
                // PeekMessage creates the thread message queue before the
                // parent may need to post WM_QUIT during startup.
                unsafe { PeekMessageW(&mut message, 0, 0, 0, 0) };
                let _ = thread_id_tx.send(thread_id);
                let result = if use_hook {
                    run_low_level_hook(entries, thread_event_tx, startup_tx)
                } else {
                    run_registered_hotkeys(entries, thread_event_tx, startup_tx)
                };
                if let Err(error) = result {
                    eprintln!("[hotkey] {error}");
                }
            });

            let windows_thread_id = match thread_id_rx.recv_timeout(Duration::from_secs(2)) {
                Ok(thread_id) => thread_id,
                Err(_) => {
                    drop(event_tx);
                    let _ = windows_thread.join();
                    let _ = dispatcher_thread.join();
                    return Err(HotkeyError::StartupTimeout);
                }
            };
            let windows_thread_id = match startup_rx.recv_timeout(Duration::from_secs(2)) {
                Ok(Ok(_)) => windows_thread_id,
                Ok(Err(error)) => {
                    drop(event_tx);
                    let _ = windows_thread.join();
                    let _ = dispatcher_thread.join();
                    return Err(error);
                }
                Err(_) => {
                    unsafe { PostThreadMessageW(windows_thread_id, WM_QUIT, 0, 0) };
                    drop(event_tx);
                    let _ = windows_thread.join();
                    let _ = dispatcher_thread.join();
                    return Err(HotkeyError::StartupTimeout);
                }
            };

            *running = Some(Running {
                windows_thread_id,
                windows_thread,
                event_tx,
                dispatcher_thread,
            });
            Ok(())
        }

        fn close(&self) -> Result<(), HotkeyError> {
            let Some(running) = self.running.lock().expect("hotkey state poisoned").take() else {
                return Ok(());
            };

            // SAFETY: the ID was returned by GetCurrentThreadId after the
            // message queue had been created by Win32 registration/hook calls.
            let posted = unsafe { PostThreadMessageW(running.windows_thread_id, WM_QUIT, 0, 0) };
            let post_error = (posted == 0).then(|| {
                HotkeyError::Platform(format!(
                    "PostThreadMessageW failed: {}",
                    io::Error::last_os_error()
                ))
            });
            running
                .windows_thread
                .join()
                .map_err(|_| HotkeyError::Platform("hotkey thread panicked".to_owned()))?;
            drop(running.event_tx);
            running
                .dispatcher_thread
                .join()
                .map_err(|_| HotkeyError::Platform("hotkey dispatcher panicked".to_owned()))?;
            post_error.map_or(Ok(()), Err)
        }
    }

    impl Drop for SystemHotkeyService {
        fn drop(&mut self) {
            let _ = self.close();
        }
    }

    fn emit(event_tx: &SyncSender<Event>, id: i32) {
        let event = if id == STOP_HOTKEY_ID {
            Event::Stop
        } else {
            Event::Task(id)
        };
        match event_tx.try_send(event) {
            Ok(()) | Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => {}
        }
    }

    fn run_registered_hotkeys(
        entries: Vec<TaskSpec>,
        event_tx: SyncSender<Event>,
        startup_tx: SyncSender<Result<u32, HotkeyError>>,
    ) -> Result<(), HotkeyError> {
        let mut registered = Vec::new();
        for entry in &entries {
            // Deliberately pass only the parsed modifiers: the compatibility
            // contract forbids adding MOD_NOREPEAT.
            let result = unsafe {
                RegisterHotKey(
                    0,
                    entry.id,
                    entry.parsed.modifiers,
                    entry.parsed.virtual_key,
                )
            };
            if result == 0 {
                for id in &registered {
                    unsafe { UnregisterHotKey(0, *id) };
                }
                let error = HotkeyError::Platform(format!(
                    "RegisterHotKey failed for id={}: {}",
                    entry.id,
                    io::Error::last_os_error()
                ));
                let _ = startup_tx.send(Err(error));
                return Ok(());
            }
            registered.push(entry.id);
        }

        let thread_id = unsafe { GetCurrentThreadId() };
        let _ = startup_tx.send(Ok(thread_id));
        let mut message = Message::default();
        loop {
            let result = unsafe { GetMessageW(&mut message, 0, 0, 0) };
            if result == 0 {
                break;
            }
            if result < 0 {
                break;
            }
            if message.message == WM_HOTKEY {
                emit(&event_tx, message.w_param as i32);
            }
        }
        for id in registered {
            unsafe { UnregisterHotKey(0, id) };
        }
        Ok(())
    }

    struct HookState {
        by_virtual_key: HashMap<u32, Vec<TaskSpec>>,
        blocked: Mutex<HashSet<u32>>,
        event_tx: SyncSender<Event>,
    }

    static ACTIVE_HOOK: Mutex<Option<Arc<HookState>>> = Mutex::new(None);

    fn run_low_level_hook(
        entries: Vec<TaskSpec>,
        event_tx: SyncSender<Event>,
        startup_tx: SyncSender<Result<u32, HotkeyError>>,
    ) -> Result<(), HotkeyError> {
        let mut by_virtual_key: HashMap<u32, Vec<TaskSpec>> = HashMap::new();
        for entry in entries {
            by_virtual_key
                .entry(entry.parsed.virtual_key)
                .or_default()
                .push(entry);
        }
        let state = Arc::new(HookState {
            by_virtual_key,
            blocked: Mutex::new(HashSet::new()),
            event_tx,
        });
        {
            let mut active = ACTIVE_HOOK.lock().expect("active hook state poisoned");
            if active.is_some() {
                let _ = startup_tx.send(Err(HotkeyError::AlreadyRunning));
                return Ok(());
            }
            *active = Some(Arc::clone(&state));
        }

        let hook =
            unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(low_level_keyboard_proc), 0, 0) };
        if hook == 0 {
            *ACTIVE_HOOK.lock().expect("active hook state poisoned") = None;
            let error = HotkeyError::Platform(format!(
                "SetWindowsHookExW failed: {}",
                io::Error::last_os_error()
            ));
            let _ = startup_tx.send(Err(error));
            return Ok(());
        }
        let thread_id = unsafe { GetCurrentThreadId() };
        let _ = startup_tx.send(Ok(thread_id));
        let mut message = Message::default();
        loop {
            let result = unsafe { GetMessageW(&mut message, 0, 0, 0) };
            if result <= 0 {
                break;
            }
        }
        unsafe { UnhookWindowsHookEx(hook) };
        *ACTIVE_HOOK.lock().expect("active hook state poisoned") = None;
        Ok(())
    }

    unsafe extern "system" fn low_level_keyboard_proc(
        code: i32,
        w_param: usize,
        l_param: isize,
    ) -> isize {
        let state = ACTIVE_HOOK
            .lock()
            .expect("active hook state poisoned")
            .clone();
        let Some(state) = state else {
            return unsafe { CallNextHookEx(0, code, w_param, l_param) };
        };
        if code < 0 || l_param == 0 {
            return unsafe { CallNextHookEx(0, code, w_param, l_param) };
        }

        let keyboard = unsafe { &*(l_param as *const LowLevelKeyboard) };
        if keyboard.flags & LLKHF_INJECTED != 0 {
            return unsafe { CallNextHookEx(0, code, w_param, l_param) };
        }

        match w_param as u32 {
            WM_KEYDOWN | WM_SYSKEYDOWN => {
                if let Some(entries) = state.by_virtual_key.get(&keyboard.virtual_key) {
                    for entry in entries {
                        if modifiers_match(entry.parsed.modifiers) {
                            state
                                .blocked
                                .lock()
                                .expect("blocked key state poisoned")
                                .insert(keyboard.virtual_key);
                            emit(&state.event_tx, entry.id);
                            return 1;
                        }
                    }
                }
            }
            WM_KEYUP | WM_SYSKEYUP
                if state
                    .blocked
                    .lock()
                    .expect("blocked key state poisoned")
                    .remove(&keyboard.virtual_key) =>
            {
                return 1;
            }
            _ => {}
        }
        unsafe { CallNextHookEx(0, code, w_param, l_param) }
    }

    fn modifiers_match(required: u32) -> bool {
        let is_down = |virtual_key| unsafe { GetAsyncKeyState(virtual_key) as u16 & 0x8000 != 0 };
        (required & MOD_ALT == 0 || is_down(VK_MENU))
            && (required & MOD_CONTROL == 0 || is_down(VK_CONTROL))
            && (required & MOD_SHIFT == 0 || is_down(VK_SHIFT))
            && (required & MOD_WIN == 0 || is_down(VK_LWIN) || is_down(VK_RWIN))
    }

    #[repr(C)]
    #[derive(Default)]
    struct Point {
        x: i32,
        y: i32,
    }

    #[repr(C)]
    #[derive(Default)]
    struct Message {
        window: isize,
        message: u32,
        w_param: usize,
        l_param: isize,
        time: u32,
        point: Point,
        private: u32,
    }

    #[repr(C)]
    struct LowLevelKeyboard {
        virtual_key: u32,
        scan_code: u32,
        flags: u32,
        time: u32,
        extra_info: usize,
    }

    #[link(name = "user32")]
    unsafe extern "system" {
        fn RegisterHotKey(window: isize, id: i32, modifiers: u32, virtual_key: u32) -> i32;
        fn UnregisterHotKey(window: isize, id: i32) -> i32;
        fn GetMessageW(message: *mut Message, window: isize, min: u32, max: u32) -> i32;
        fn PeekMessageW(
            message: *mut Message,
            window: isize,
            min: u32,
            max: u32,
            remove: u32,
        ) -> i32;
        fn PostThreadMessageW(thread_id: u32, message: u32, w_param: usize, l_param: isize) -> i32;
        fn SetWindowsHookExW(
            hook: i32,
            callback: Option<unsafe extern "system" fn(i32, usize, isize) -> isize>,
            module: isize,
            thread_id: u32,
        ) -> isize;
        fn UnhookWindowsHookEx(hook: isize) -> i32;
        fn CallNextHookEx(hook: isize, code: i32, w_param: usize, l_param: isize) -> isize;
        fn GetAsyncKeyState(virtual_key: i32) -> i16;
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentThreadId() -> u32;
    }
}

#[cfg(windows)]
pub use implementation::SystemHotkeyService;

#[cfg(not(windows))]
pub struct SystemHotkeyService {
    _options: super::Options,
}

#[cfg(not(windows))]
impl SystemHotkeyService {
    pub fn new(options: super::Options) -> Self {
        Self { _options: options }
    }
}

#[cfg(not(windows))]
impl super::Service for SystemHotkeyService {
    fn start(&self, _handler: super::Handler) -> Result<(), super::HotkeyError> {
        Err(super::HotkeyError::Unsupported)
    }

    fn close(&self) -> Result<(), super::HotkeyError> {
        Ok(())
    }
}
