use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use stp::clipboard::{Clipboard, ClipboardError, Manager, Sleeper, TextIo};
use stp::keyboard::{KeySimulator, KeyboardError};

const DEFAULT_WRITE_DELAY: Duration = Duration::from_millis(80);
const DEFAULT_RESTORE_DELAY: Duration = Duration::from_millis(120);

struct FakeClipboard {
    reads: Mutex<VecDeque<Result<String, ClipboardError>>>,
    writes: Mutex<Vec<String>>,
    fail_writes: Mutex<usize>,
}

impl FakeClipboard {
    fn new(
        reads: impl IntoIterator<Item = Result<String, ClipboardError>>,
        fail_writes: usize,
    ) -> Self {
        Self {
            reads: Mutex::new(reads.into_iter().collect()),
            writes: Mutex::new(Vec::new()),
            fail_writes: Mutex::new(fail_writes),
        }
    }

    fn writes(&self) -> Vec<String> {
        self.writes.lock().unwrap().clone()
    }
}

impl Clipboard for FakeClipboard {
    fn read_all(&self) -> Result<String, ClipboardError> {
        self.reads
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| Ok(String::new()))
    }

    fn write_all(&self, text: &str) -> Result<(), ClipboardError> {
        self.writes.lock().unwrap().push(text.to_owned());
        let mut remaining = self.fail_writes.lock().unwrap();
        if *remaining > 0 {
            *remaining -= 1;
            Err(ClipboardError::Platform("write failed".to_owned()))
        } else {
            Ok(())
        }
    }
}

#[derive(Default)]
struct FakeKeyboard {
    copy_calls: Mutex<usize>,
    paste_calls: Mutex<usize>,
}

impl KeySimulator for FakeKeyboard {
    fn copy(&self) -> Result<(), KeyboardError> {
        *self.copy_calls.lock().unwrap() += 1;
        Ok(())
    }

    fn paste(&self) -> Result<(), KeyboardError> {
        *self.paste_calls.lock().unwrap() += 1;
        Ok(())
    }
}

#[derive(Default)]
struct FakeSleeper {
    calls: Mutex<Vec<Duration>>,
}

impl Sleeper for FakeSleeper {
    fn sleep(&self, duration: Duration) {
        self.calls.lock().unwrap().push(duration);
    }
}

impl FakeSleeper {
    fn calls(&self) -> Vec<Duration> {
        self.calls.lock().unwrap().clone()
    }
}

#[test]
fn copy_uses_current_retry_poll_and_restore_order() {
    let clipboard = Arc::new(FakeClipboard::new(
        [
            Ok("original".to_owned()),
            Ok(String::new()),
            Ok("selected".to_owned()),
        ],
        2,
    ));
    let keyboard = Arc::new(FakeKeyboard::default());
    let sleeper = Arc::new(FakeSleeper::default());
    let manager = Manager::with_sleeper(
        clipboard.clone(),
        keyboard.clone(),
        sleeper,
        Duration::from_secs(1),
        DEFAULT_WRITE_DELAY,
        DEFAULT_RESTORE_DELAY,
    );

    assert_eq!(manager.copy_selected().unwrap(), "selected");
    assert_eq!(clipboard.writes(), vec!["", "", "", "original"]);
    assert_eq!(*keyboard.copy_calls.lock().unwrap(), 1);
}

#[test]
fn paste_retries_write_then_restores_original() {
    let clipboard = Arc::new(FakeClipboard::new([Ok("original".to_owned())], 2));
    let keyboard = Arc::new(FakeKeyboard::default());
    let sleeper = Arc::new(FakeSleeper::default());
    let write_delay = Duration::from_millis(17);
    let restore_delay = Duration::from_millis(29);
    let manager = Manager::with_sleeper(
        clipboard.clone(),
        keyboard.clone(),
        sleeper.clone(),
        Duration::from_secs(1),
        write_delay,
        restore_delay,
    );

    manager.paste_text("result").unwrap();
    assert_eq!(
        clipboard.writes(),
        vec!["result", "result", "result", "original"]
    );
    assert_eq!(*keyboard.paste_calls.lock().unwrap(), 1);
    assert_eq!(
        sleeper.calls(),
        vec![
            Duration::from_millis(50),
            Duration::from_millis(50),
            write_delay,
            restore_delay,
        ]
    );
}

#[test]
fn copy_continues_after_five_clear_failures_and_still_restores() {
    let clipboard = Arc::new(FakeClipboard::new([Ok("original".to_owned())], 5));
    let keyboard = Arc::new(FakeKeyboard::default());
    let manager = Manager::with_sleeper(
        clipboard.clone(),
        keyboard.clone(),
        Arc::new(FakeSleeper::default()),
        Duration::from_secs(1),
        DEFAULT_WRITE_DELAY,
        DEFAULT_RESTORE_DELAY,
    );

    assert!(manager.copy_selected().is_err());
    assert_eq!(clipboard.writes(), vec!["", "", "", "", "", "original"]);
    assert_eq!(*keyboard.copy_calls.lock().unwrap(), 1);
}
