//! The vocabulary keymaps are written in: HID keyboard usages by name,
//! modifiers, and HID Consumer (media) usages.
//!
//! An *accord* is one step of a key sequence: modifiers plus at most one key,
//! written `ctrl-shift-a`. A lone modifier (`ctrl`) is allowed, and `<104>`
//! names a key by its decimal HID usage. This is the syntax of
//! ch57x-keyboard-tool, which every driver accepts.

use anyhow::{Result, anyhow, ensure};

/// HID keyboard usages from 0x04 upwards, in order.
pub const KEY_NAMES: &[&str] = &[
    "a",
    "b",
    "c",
    "d",
    "e",
    "f",
    "g",
    "h",
    "i",
    "j",
    "k",
    "l",
    "m",
    "n",
    "o",
    "p",
    "q",
    "r",
    "s",
    "t",
    "u",
    "v",
    "w",
    "x",
    "y",
    "z",
    "1",
    "2",
    "3",
    "4",
    "5",
    "6",
    "7",
    "8",
    "9",
    "0",
    "enter",
    "escape",
    "backspace",
    "tab",
    "space",
    "minus",
    "equal",
    "leftbracket",
    "rightbracket",
    "backslash",
    "nonushash",
    "semicolon",
    "quote",
    "grave",
    "comma",
    "dot",
    "slash",
    "capslock",
    "f1",
    "f2",
    "f3",
    "f4",
    "f5",
    "f6",
    "f7",
    "f8",
    "f9",
    "f10",
    "f11",
    "f12",
    "printscreen",
    "scrolllock",
    "pause",
    "insert",
    "home",
    "pageup",
    "delete",
    "end",
    "pagedown",
    "right",
    "left",
    "down",
    "up",
    "numlock",
    "numpadslash",
    "numpadasterisk",
    "numpadminus",
    "numpadplus",
    "numpadenter",
    "numpad1",
    "numpad2",
    "numpad3",
    "numpad4",
    "numpad5",
    "numpad6",
    "numpad7",
    "numpad8",
    "numpad9",
    "numpad0",
    "numpaddot",
    "nonusbackslash",
    "application",
    "power",
    "numpadequal",
    "f13",
    "f14",
    "f15",
    "f16",
    "f17",
    "f18",
    "f19",
    "f20",
    "f21",
    "f22",
    "f23",
    "f24",
];

const KEY_ALIASES: &[(&str, &str)] = &[
    ("esc", "escape"),
    ("macbrightnessdown", "scrolllock"),
    ("macbrightnessup", "pause"),
];

/// Modifier names and their HID bits. `win`, `cmd` and `super` are one key.
pub const MODIFIERS: &[(&str, u8)] = &[
    ("ctrl", 0x01),
    ("shift", 0x02),
    ("alt", 0x04),
    ("opt", 0x04),
    ("win", 0x08),
    ("cmd", 0x08),
    ("super", 0x08),
    ("rctrl", 0x10),
    ("rshift", 0x20),
    ("ralt", 0x40),
    ("ropt", 0x40),
    ("rwin", 0x80),
    ("rcmd", 0x80),
    ("rsuper", 0x80),
];

/// The modifier names the editor offers, one per bit.
pub const MODIFIER_NAMES: &[&str] = &[
    "ctrl", "shift", "alt", "win", "rctrl", "rshift", "ralt", "rwin",
];

/// Named media keys and their HID Consumer usages.
pub const MEDIA: &[(&str, u16)] = &[
    ("screenbrightnessup", 0x6F),
    ("screenbrightnessdown", 0x70),
    ("next", 0xB5),
    ("previous", 0xB6),
    ("stop", 0xB7),
    ("play", 0xCD),
    ("mute", 0xE2),
    ("volumeup", 0xE9),
    ("volumedown", 0xEA),
    ("favorites", 0x182),
    ("calculator", 0x192),
    ("screenlock", 0x19E),
    ("webpagehome", 0x223),
    ("webpageback", 0x224),
    ("webpageforward", 0x225),
];

pub fn key_code(name: &str) -> Option<u8> {
    let name = KEY_ALIASES
        .iter()
        .find(|(a, _)| *a == name)
        .map_or(name, |(_, k)| k);
    KEY_NAMES
        .iter()
        .position(|k| *k == name)
        .map(|i| 0x04 + i as u8)
}

pub fn key_name(code: u8) -> Option<&'static str> {
    KEY_NAMES.get(code.checked_sub(0x04)? as usize).copied()
}

pub fn modifier_bits(name: &str) -> Option<u8> {
    MODIFIERS.iter().find(|(n, _)| *n == name).map(|(_, b)| *b)
}

pub fn media_code(name: &str) -> Option<u16> {
    let name = if name == "prev" { "previous" } else { name };
    MEDIA.iter().find(|(n, _)| *n == name).map(|(_, c)| *c)
}

/// Folds right-hand modifier bits onto the left ones.
pub fn fold_mods(mods: u8) -> u8 {
    (mods | (mods >> 4)) & 0x0F
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Accord {
    /// HID modifier bits.
    pub mods: u8,
    /// HID keyboard usage, or `None` for modifiers alone.
    pub code: Option<u8>,
}

impl Accord {
    /// Writes the accord back in the syntax [`parse_accord`] reads.
    pub fn to_expr(self) -> String {
        let mut parts: Vec<String> = MODIFIER_NAMES
            .iter()
            .filter(|n| self.mods & modifier_bits(n).unwrap_or(0) != 0)
            .map(|n| n.to_string())
            .collect();
        if let Some(code) = self.code {
            parts.push(key_name(code).map_or_else(|| format!("<{code}>"), str::to_string));
        }
        parts.join("-")
    }
}

pub fn parse_accord(s: &str) -> Result<Accord> {
    let s = s.trim().to_lowercase();
    ensure!(!s.is_empty(), "empty key");
    let parts: Vec<&str> = s.split('-').collect();
    let (last, mods) = parts.split_last().expect("split yields at least one part");
    let mut bits = 0;
    for m in mods {
        bits |= modifier_bits(m).ok_or_else(|| anyhow!("unknown modifier {m:?} in {s:?}"))?;
    }
    if let Some(n) = last.strip_prefix('<').and_then(|r| r.strip_suffix('>')) {
        let code = n
            .parse::<u8>()
            .map_err(|_| anyhow!("custom key code out of range in {s:?}"))?;
        return Ok(Accord {
            mods: bits,
            code: Some(code),
        });
    }
    if let Some(code) = key_code(last) {
        return Ok(Accord {
            mods: bits,
            code: Some(code),
        });
    }
    if let Some(m) = modifier_bits(last) {
        return Ok(Accord {
            mods: bits | m,
            code: None,
        });
    }
    Err(anyhow!("unknown key {last:?} in {s:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_table_spans_a_to_f24() {
        assert_eq!(key_code("a"), Some(0x04));
        assert_eq!(key_code("0"), Some(0x27));
        assert_eq!(key_code("f12"), Some(0x45));
        assert_eq!(key_code("f13"), Some(0x68));
        assert_eq!(key_code("f24"), Some(0x73));
        assert_eq!(key_code("esc"), key_code("escape"));
        for (i, name) in KEY_NAMES.iter().enumerate() {
            assert_eq!(key_name(0x04 + i as u8), Some(*name));
        }
    }

    #[test]
    fn accords() {
        assert_eq!(
            parse_accord("ctrl-shift-a").unwrap(),
            Accord {
                mods: 0x03,
                code: Some(0x04)
            }
        );
        assert_eq!(
            parse_accord("CMD-c").unwrap(),
            parse_accord("win-c").unwrap()
        );
        assert_eq!(
            parse_accord("ctrl-alt").unwrap(),
            Accord {
                mods: 0x05,
                code: None
            }
        );
        assert_eq!(parse_accord("<104>").unwrap().code, Some(104));
        assert!(parse_accord("hyper-a").is_err());
        assert!(parse_accord("ctrl-nosuchkey").is_err());
        let a = parse_accord("ctrl-alt-shift-f16").unwrap();
        assert_eq!(parse_accord(&a.to_expr()).unwrap(), a);
    }
}
