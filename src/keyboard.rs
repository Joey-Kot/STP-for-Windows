// Copyright (C) 2026 Joey Kot <joey.kot.x@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later

use thiserror::Error;

const KEYEVENTF_KEYUP: u32 = 0x0002;
const KEYEVENTF_SCANCODE: u32 = 0x0008;
const VIRTUAL_KEY_OFFSET: i32 = 0x0fff;
const VK_CTRL_KEYBD_EVENT: i32 = 0x11 + VIRTUAL_KEY_OFFSET;
const SCAN_C: i32 = 46;
const SCAN_V: i32 = 47;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyEvent {
    pub b_vk: u8,
    pub b_scan: u8,
    pub flags: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Key {
    Copy,
    Paste,
}

fn down_event(mut key: i32) -> KeyEvent {
    let mut flags = 0;
    if key < VIRTUAL_KEY_OFFSET {
        flags |= KEYEVENTF_SCANCODE;
    } else {
        key -= VIRTUAL_KEY_OFFSET;
    }
    KeyEvent {
        b_vk: key as u8,
        b_scan: (key + 0x80) as u8,
        flags,
    }
}

fn up_event(mut key: i32) -> KeyEvent {
    let mut flags = KEYEVENTF_KEYUP;
    if key < VIRTUAL_KEY_OFFSET {
        flags |= KEYEVENTF_SCANCODE;
    } else {
        key -= VIRTUAL_KEY_OFFSET;
    }
    KeyEvent {
        b_vk: key as u8,
        b_scan: (key + 0x80) as u8,
        flags,
    }
}

fn chord_events(key: Key) -> [KeyEvent; 4] {
    let scan = match key {
        Key::Copy => SCAN_C,
        Key::Paste => SCAN_V,
    };
    // This deliberately mirrors micmonay/keybd_event v1.1.2, including its
    // modifier-first release order.
    [
        down_event(VK_CTRL_KEYBD_EVENT),
        down_event(scan),
        up_event(VK_CTRL_KEYBD_EVENT),
        up_event(scan),
    ]
}

#[derive(Debug, Error)]
pub enum KeyboardError {
    #[error("keyboard injection is only supported on Windows")]
    Unsupported,
}

pub trait KeySimulator: Send + Sync {
    fn copy(&self) -> Result<(), KeyboardError>;
    fn paste(&self) -> Result<(), KeyboardError>;
}

#[derive(Default)]
pub struct SystemKeySimulator;

impl SystemKeySimulator {
    pub fn new() -> Self {
        Self
    }

    fn launch(&self, key: Key) -> Result<(), KeyboardError> {
        launch_platform(chord_events(key))
    }
}

impl KeySimulator for SystemKeySimulator {
    fn copy(&self) -> Result<(), KeyboardError> {
        self.launch(Key::Copy)
    }

    fn paste(&self) -> Result<(), KeyboardError> {
        self.launch(Key::Paste)
    }
}

#[cfg(windows)]
fn launch_platform(events: [KeyEvent; 4]) -> Result<(), KeyboardError> {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn keybd_event(b_vk: u8, b_scan: u8, flags: u32, extra_info: usize);
    }

    for event in events {
        // SAFETY: keybd_event is a process-local Win32 call and all values are
        // copied scalars computed exactly as keybd_event v1.1.2 computes them.
        unsafe { keybd_event(event.b_vk, event.b_scan, event.flags, 0) };
    }
    Ok(())
}

#[cfg(not(windows))]
fn launch_platform(_events: [KeyEvent; 4]) -> Result<(), KeyboardError> {
    Err(KeyboardError::Unsupported)
}

#[cfg(test)]
mod tests {
    use super::{KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE, Key, KeyEvent, chord_events};

    #[test]
    fn copy_matches_keybd_event_v1_1_2() {
        assert_eq!(
            chord_events(Key::Copy),
            [
                KeyEvent {
                    b_vk: 0x11,
                    b_scan: 0x91,
                    flags: 0
                },
                KeyEvent {
                    b_vk: 46,
                    b_scan: 174,
                    flags: KEYEVENTF_SCANCODE
                },
                KeyEvent {
                    b_vk: 0x11,
                    b_scan: 0x91,
                    flags: KEYEVENTF_KEYUP
                },
                KeyEvent {
                    b_vk: 46,
                    b_scan: 174,
                    flags: KEYEVENTF_KEYUP | KEYEVENTF_SCANCODE,
                },
            ]
        );
    }

    #[test]
    fn paste_matches_keybd_event_v1_1_2() {
        let events = chord_events(Key::Paste);
        assert_eq!(events[1].b_vk, 47);
        assert_eq!(events[1].b_scan, 175);
        assert_eq!(events[2].b_vk, 0x11);
        assert_eq!(events[3].b_vk, 47);
    }
}
