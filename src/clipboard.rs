// Copyright (C) 2026 Joey Kot <joey.kot.x@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later

use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use thiserror::Error;

use crate::keyboard::{KeySimulator, KeyboardError};

const ATTEMPTS: usize = 5;
const RETRY_DELAY: Duration = Duration::from_millis(50);
const COPY_SETTLE_DELAY: Duration = Duration::from_millis(50);
const COPY_POLL_DELAY: Duration = Duration::from_millis(50);
const COPY_RESTORE_DELAY: Duration = Duration::from_millis(150);

#[derive(Debug, Error)]
pub enum ClipboardError {
    #[error("clipboard is only supported on Windows")]
    Unsupported,
    #[error("{0}")]
    Platform(String),
}

#[derive(Debug, Error)]
pub enum TextIoError {
    #[error(transparent)]
    Clipboard(#[from] ClipboardError),
    #[error(transparent)]
    Keyboard(#[from] KeyboardError),
    #[error("timeout waiting for clipboard after Ctrl+C")]
    CopyTimeout,
    #[error("failed to write clipboard")]
    WriteFailed,
}

pub trait Clipboard: Send + Sync {
    fn read_all(&self) -> Result<String, ClipboardError>;
    fn write_all(&self, text: &str) -> Result<(), ClipboardError>;
}

pub trait Sleeper: Send + Sync {
    fn sleep(&self, duration: Duration);
}

#[derive(Default)]
pub struct SystemSleeper;

impl Sleeper for SystemSleeper {
    fn sleep(&self, duration: Duration) {
        thread::sleep(duration);
    }
}

pub trait TextIo: Send + Sync {
    fn copy_selected(&self) -> Result<String, TextIoError>;
    fn paste_text(&self, text: &str) -> Result<(), TextIoError>;
}

pub struct Manager {
    clipboard: Arc<dyn Clipboard>,
    keyboard: Arc<dyn KeySimulator>,
    sleeper: Arc<dyn Sleeper>,
    timeout: Duration,
    write_delay: Duration,
    restore_delay: Duration,
}

impl Manager {
    pub fn new(
        clipboard: Arc<dyn Clipboard>,
        keyboard: Arc<dyn KeySimulator>,
        timeout: Duration,
        write_delay: Duration,
        restore_delay: Duration,
    ) -> Self {
        Self::with_sleeper(
            clipboard,
            keyboard,
            Arc::new(SystemSleeper),
            timeout,
            write_delay,
            restore_delay,
        )
    }

    pub fn with_sleeper(
        clipboard: Arc<dyn Clipboard>,
        keyboard: Arc<dyn KeySimulator>,
        sleeper: Arc<dyn Sleeper>,
        timeout: Duration,
        write_delay: Duration,
        restore_delay: Duration,
    ) -> Self {
        Self {
            clipboard,
            keyboard,
            sleeper,
            timeout,
            write_delay,
            restore_delay,
        }
    }

    fn restore_after(&self, original: &str, delay: Duration) {
        self.sleeper.sleep(delay);
        for _ in 0..ATTEMPTS {
            if self.clipboard.write_all(original).is_ok() {
                break;
            }
            self.sleeper.sleep(RETRY_DELAY);
        }
    }
}

impl TextIo for Manager {
    fn copy_selected(&self) -> Result<String, TextIoError> {
        let original = self.clipboard.read_all().unwrap_or_default();
        let result = (|| {
            for _ in 0..ATTEMPTS {
                if self.clipboard.write_all("").is_ok() {
                    break;
                }
                self.sleeper.sleep(RETRY_DELAY);
            }
            self.sleeper.sleep(COPY_SETTLE_DELAY);
            self.keyboard.copy()?;

            let started = Instant::now();
            loop {
                let elapsed = started.elapsed();
                if elapsed >= self.timeout {
                    return Err(TextIoError::CopyTimeout);
                }
                self.sleeper
                    .sleep(COPY_POLL_DELAY.min(self.timeout - elapsed));
                if let Ok(text) = self.clipboard.read_all()
                    && !text.trim().is_empty()
                {
                    return Ok(text);
                }
            }
        })();
        self.restore_after(&original, COPY_RESTORE_DELAY);
        result
    }

    fn paste_text(&self, text: &str) -> Result<(), TextIoError> {
        let original = self.clipboard.read_all().unwrap_or_default();
        let result = (|| {
            for _ in 0..ATTEMPTS {
                if self.clipboard.write_all(text).is_ok() {
                    self.sleeper.sleep(self.write_delay);
                    return self.keyboard.paste().map_err(TextIoError::from);
                }
                self.sleeper.sleep(RETRY_DELAY);
            }
            Err(TextIoError::WriteFailed)
        })();
        self.restore_after(&original, self.restore_delay);
        result
    }
}

#[derive(Default)]
pub struct SystemClipboard;

impl SystemClipboard {
    pub fn new() -> Self {
        Self
    }
}

impl Clipboard for SystemClipboard {
    fn read_all(&self) -> Result<String, ClipboardError> {
        platform::read_all()
    }

    fn write_all(&self, text: &str) -> Result<(), ClipboardError> {
        platform::write_all(text)
    }
}

#[cfg(windows)]
mod platform {
    use std::ffi::c_void;
    use std::io;
    use std::ptr;
    use std::slice;
    use std::thread;
    use std::time::{Duration, Instant};

    use super::ClipboardError;

    const CF_UNICODETEXT: u32 = 13;
    const GMEM_MOVEABLE: u32 = 0x0002;
    const ERROR_SUCCESS: u32 = 0;

    struct OpenClipboardGuard {
        closed: bool,
    }

    impl OpenClipboardGuard {
        fn close(mut self) -> Result<(), ClipboardError> {
            let result = unsafe { CloseClipboard() };
            self.closed = true;
            if result == 0 {
                return Err(platform_error("CloseClipboard"));
            }
            Ok(())
        }
    }

    impl Drop for OpenClipboardGuard {
        fn drop(&mut self) {
            if !self.closed {
                unsafe { CloseClipboard() };
            }
        }
    }

    struct GlobalMemory(isize);

    impl GlobalMemory {
        fn release(mut self) -> isize {
            let handle = self.0;
            self.0 = 0;
            handle
        }
    }

    impl Drop for GlobalMemory {
        fn drop(&mut self) {
            if self.0 != 0 {
                unsafe { GlobalFree(self.0) };
            }
        }
    }

    struct GlobalLockGuard {
        handle: isize,
        pointer: *mut c_void,
    }

    impl Drop for GlobalLockGuard {
        fn drop(&mut self) {
            unsafe {
                SetLastError(ERROR_SUCCESS);
                let result = GlobalUnlock(self.handle);
                let error = GetLastError();
                if result == 0 && error != ERROR_SUCCESS {
                    // Drop cannot report this error. Explicit callers unlock
                    // before ownership transfer where the result matters.
                }
            }
        }
    }

    fn platform_error(operation: &str) -> ClipboardError {
        ClipboardError::Platform(format!(
            "{operation} failed: {}",
            io::Error::last_os_error()
        ))
    }

    fn open_clipboard() -> Result<OpenClipboardGuard, ClipboardError> {
        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            if unsafe { OpenClipboard(0) } != 0 {
                return Ok(OpenClipboardGuard { closed: false });
            }
            if Instant::now() >= deadline {
                return Err(platform_error("OpenClipboard"));
            }
            thread::sleep(Duration::from_millis(1));
        }
    }

    fn lock_global(handle: isize) -> Result<GlobalLockGuard, ClipboardError> {
        let pointer = unsafe { GlobalLock(handle) };
        if pointer.is_null() {
            return Err(platform_error("GlobalLock"));
        }
        Ok(GlobalLockGuard { handle, pointer })
    }

    pub fn read_all() -> Result<String, ClipboardError> {
        if unsafe { IsClipboardFormatAvailable(CF_UNICODETEXT) } == 0 {
            return Err(platform_error("IsClipboardFormatAvailable"));
        }
        let clipboard = open_clipboard()?;
        let handle = unsafe { GetClipboardData(CF_UNICODETEXT) };
        if handle == 0 {
            return Err(platform_error("GetClipboardData"));
        }
        let locked = lock_global(handle)?;
        let pointer = locked.pointer.cast::<u16>();
        let mut length = 0usize;
        // CF_UNICODETEXT is required to be NUL-terminated. This matches the
        // prior implementation's NUL scan while avoiding a fixed 1 MiB slice.
        while unsafe { *pointer.add(length) } != 0 {
            length += 1;
        }
        let text = String::from_utf16_lossy(unsafe { slice::from_raw_parts(pointer, length) });
        drop(locked);
        clipboard.close()?;
        Ok(text)
    }

    pub fn write_all(text: &str) -> Result<(), ClipboardError> {
        let clipboard = open_clipboard()?;
        if unsafe { EmptyClipboard() } == 0 {
            return Err(platform_error("EmptyClipboard"));
        }

        let encoded: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        let byte_length = encoded.len() * size_of::<u16>();
        let handle = unsafe { GlobalAlloc(GMEM_MOVEABLE, byte_length) };
        if handle == 0 {
            return Err(platform_error("GlobalAlloc"));
        }
        let memory = GlobalMemory(handle);
        let locked = lock_global(handle)?;
        unsafe {
            ptr::copy_nonoverlapping(
                encoded.as_ptr().cast::<u8>(),
                locked.pointer.cast::<u8>(),
                byte_length,
            );
            SetLastError(ERROR_SUCCESS);
            let unlocked = GlobalUnlock(handle);
            let error = GetLastError();
            std::mem::forget(locked);
            if unlocked == 0 && error != ERROR_SUCCESS {
                return Err(platform_error("GlobalUnlock"));
            }
        }

        if unsafe { SetClipboardData(CF_UNICODETEXT, handle) } == 0 {
            return Err(platform_error("SetClipboardData"));
        }
        let _ = memory.release();
        clipboard.close()?;
        Ok(())
    }

    #[link(name = "user32")]
    unsafe extern "system" {
        fn IsClipboardFormatAvailable(format: u32) -> i32;
        fn OpenClipboard(owner: isize) -> i32;
        fn CloseClipboard() -> i32;
        fn EmptyClipboard() -> i32;
        fn GetClipboardData(format: u32) -> isize;
        fn SetClipboardData(format: u32, memory: isize) -> isize;
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GlobalAlloc(flags: u32, bytes: usize) -> isize;
        fn GlobalFree(memory: isize) -> isize;
        fn GlobalLock(memory: isize) -> *mut c_void;
        fn GlobalUnlock(memory: isize) -> i32;
        fn GetLastError() -> u32;
        fn SetLastError(error: u32);
    }
}

#[cfg(not(windows))]
mod platform {
    use super::ClipboardError;

    pub fn read_all() -> Result<String, ClipboardError> {
        Err(ClipboardError::Unsupported)
    }

    pub fn write_all(_text: &str) -> Result<(), ClipboardError> {
        Err(ClipboardError::Unsupported)
    }
}
