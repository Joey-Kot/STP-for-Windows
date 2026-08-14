// Copyright (C) 2026 Joey Kot <joey.kot.x@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later

use thiserror::Error;

pub const MOD_ALT: u32 = 0x0001;
pub const MOD_CONTROL: u32 = 0x0002;
pub const MOD_SHIFT: u32 = 0x0004;
pub const MOD_WIN: u32 = 0x0008;

pub const VK_NUMPAD0: u32 = 0x60;
pub const VK_NUMPAD1: u32 = 0x61;
pub const VK_NUMPAD2: u32 = 0x62;
pub const VK_NUMPAD3: u32 = 0x63;
pub const VK_NUMPAD4: u32 = 0x64;
pub const VK_NUMPAD5: u32 = 0x65;
pub const VK_NUMPAD6: u32 = 0x66;
pub const VK_NUMPAD7: u32 = 0x67;
pub const VK_NUMPAD8: u32 = 0x68;
pub const VK_NUMPAD9: u32 = 0x69;
pub const VK_ADD: u32 = 0x6b;
pub const VK_SUBTRACT: u32 = 0x6d;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParsedHotkey {
    pub modifiers: u32,
    pub virtual_key: u32,
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum ParseHotkeyError {
    #[error("empty key")]
    Empty,
    #[error("unsupported key token: {0}")]
    Unsupported(String),
}

pub fn parse_hotkey(spec: &str) -> Result<ParsedHotkey, ParseHotkeyError> {
    if spec.is_empty() {
        return Err(ParseHotkeyError::Empty);
    }

    let parts: Vec<String> = spec
        .split('+')
        .map(|part| part.trim().to_ascii_lowercase())
        .collect();
    let key_token = parts.last().expect("split always returns one element");
    let mut modifiers = 0;
    for modifier in &parts[..parts.len() - 1] {
        match modifier.as_str() {
            "alt" | "menu" => modifiers |= MOD_ALT,
            "ctrl" | "control" => modifiers |= MOD_CONTROL,
            "shift" => modifiers |= MOD_SHIFT,
            "win" | "meta" | "super" => modifiers |= MOD_WIN,
            // The compatibility contract preserves ignored unknown modifiers.
            _ => {}
        }
    }

    if key_token.len() == 1 {
        let byte = key_token.as_bytes()[0];
        if byte.is_ascii_lowercase() {
            return Ok(ParsedHotkey {
                modifiers,
                virtual_key: byte.to_ascii_uppercase().into(),
            });
        }
        if byte.is_ascii_digit() {
            return Ok(ParsedHotkey {
                modifiers,
                virtual_key: byte.into(),
            });
        }
    }

    let named = match key_token.as_str() {
        "esc" | "escape" => Some(0x1b),
        "space" => Some(0x20),
        "enter" | "return" => Some(0x0d),
        "tab" => Some(0x09),
        "backspace" => Some(0x08),
        "insert" => Some(0x2d),
        "delete" => Some(0x2e),
        "home" => Some(0x24),
        "end" => Some(0x23),
        "pageup" => Some(0x21),
        "pagedown" => Some(0x22),
        "left" => Some(0x25),
        "up" => Some(0x26),
        "right" => Some(0x27),
        "down" => Some(0x28),
        "numpad0" | "num0" | "kp0" => Some(VK_NUMPAD0),
        "numpad1" | "num1" | "kp1" => Some(VK_NUMPAD1),
        "numpad2" | "num2" | "kp2" => Some(VK_NUMPAD2),
        "numpad3" | "num3" | "kp3" => Some(VK_NUMPAD3),
        "numpad4" | "num4" | "kp4" => Some(VK_NUMPAD4),
        "numpad5" | "num5" | "kp5" => Some(VK_NUMPAD5),
        "numpad6" | "num6" | "kp6" => Some(VK_NUMPAD6),
        "numpad7" | "num7" | "kp7" => Some(VK_NUMPAD7),
        "numpad8" | "num8" | "kp8" => Some(VK_NUMPAD8),
        "numpad9" | "num9" | "kp9" => Some(VK_NUMPAD9),
        "add" | "plus" | "kpadd" => Some(VK_ADD),
        "subtract" | "minus" | "kpsubtract" => Some(VK_SUBTRACT),
        _ => None,
    };
    if let Some(virtual_key) = named {
        return Ok(ParsedHotkey {
            modifiers,
            virtual_key,
        });
    }

    if let Some(number) = key_token
        .strip_prefix('f')
        .and_then(|v| v.parse::<u32>().ok())
        && (1..=24).contains(&number)
    {
        return Ok(ParsedHotkey {
            modifiers,
            virtual_key: 0x70 + number - 1,
        });
    }

    Err(ParseHotkeyError::Unsupported(spec.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::{MOD_ALT, MOD_CONTROL, MOD_SHIFT, MOD_WIN, parse_hotkey};

    #[test]
    fn parses_current_aliases() {
        assert_eq!(parse_hotkey("Control+F24").unwrap().modifiers, MOD_CONTROL);
        assert_eq!(parse_hotkey("menu+kpadd").unwrap().modifiers, MOD_ALT);
        assert_eq!(
            parse_hotkey("shift+meta+num9").unwrap().modifiers,
            MOD_SHIFT | MOD_WIN
        );
    }

    #[test]
    fn preserves_ignored_unknown_modifier_behavior() {
        let parsed = parse_hotkey("unknown+a").unwrap();
        assert_eq!(parsed.modifiers, 0);
        assert_eq!(parsed.virtual_key, u32::from(b'A'));
    }
}
