//! The standard keymap: what the editor edits and every driver reads.
//!
//! ```json
//! {"driver": "ch57x", "model": "ch57x-1", "rows": 3, "columns": 4, "knobs": 2,
//!  "layers": [{"name": "Edit", "led": "backlight white",
//!              "slots": {"b0": <action>, "k0.ccw": <action>, "k0.press": …, "k0.cw": …}}]}
//! ```
//!
//! Slots are named `b<i>` for buttons, counted row by row, and
//! `k<i>.ccw|press|cw` for knobs. Hardware actions (`keys`, `media`, `mouse`,
//! `raw`) are stored on the device; which ones a device accepts is in its
//! driver's [`crate::Capabilities`]. Software actions (`text`, `open`,
//! `shell`, `macro`) are run by the agent, see [`crate::agent`].
//!
//! This is macropad-studio's mapping format plus `driver` and `model`;
//! Studio files (no `driver`) read as CH57x mappings.

use std::collections::BTreeMap;

use anyhow::{Result, anyhow, bail};
use serde::{Deserialize, Deserializer, Serialize};

pub const KNOB_ACTIONS: [&str; 3] = ["ccw", "press", "cw"];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mapping {
    /// The [`crate::DeviceDriver::id`] this mapping is for.
    #[serde(default = "default_driver")]
    pub driver: String,
    /// One of that driver's [`crate::driver::ModelInfo::id`]s.
    #[serde(default = "default_model")]
    pub model: String,
    pub rows: u8,
    pub columns: u8,
    pub knobs: u8,
    pub layers: Vec<Layer>,
}

fn default_driver() -> String {
    "ch57x".into()
}

fn default_model() -> String {
    "ch57x-1".into()
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Layer {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Lighting for this layer, in the form the driver's
    /// [`crate::driver::LedCaps`] lists: `"<mode> <colour>"` or `"off"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub led: Option<String>,
    #[serde(default, deserialize_with = "drop_null_slots")]
    pub slots: BTreeMap<String, Action>,
}

/// macropad-studio writes `"b3": null` for a cleared slot.
fn drop_null_slots<'de, D: Deserializer<'de>>(d: D) -> Result<BTreeMap<String, Action>, D::Error> {
    let raw: Option<BTreeMap<String, Option<Action>>> = Option::deserialize(d)?;
    Ok(raw
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(k, v)| v.map(|v| (k, v)))
        .collect())
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Action {
    /// Keycap text shown in the editor; never sent to the device.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(flatten)]
    pub kind: ActionKind,
}

impl Action {
    pub fn new(kind: ActionKind) -> Self {
        Self { label: None, kind }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ActionKind {
    /// A sequence of accords (see [`crate::keys`]), with one delay between all steps.
    Keys {
        #[serde(default)]
        steps: Vec<String>,
        #[serde(default)]
        delay: u32,
    },
    /// A named media key or any HID Consumer usage (number, or "0x1b6").
    Media { code: MediaRef },
    Mouse {
        action: MouseKind,
        #[serde(default)]
        buttons: Vec<String>,
        #[serde(default)]
        modifier: String,
        #[serde(default)]
        dx: i32,
        #[serde(default)]
        dy: i32,
        #[serde(default)]
        delta: i32,
    },
    /// A driver-specific expression (ch57x-keyboard-tool syntax for CH57x).
    Raw {
        #[serde(default)]
        expr: String,
    },
    Text {
        #[serde(default)]
        text: String,
    },
    Open {
        #[serde(default)]
        target: String,
    },
    Shell {
        #[serde(default)]
        command: String,
    },
    Macro {
        #[serde(default)]
        steps: Vec<MacroStep>,
    },
}

impl ActionKind {
    /// Run by the agent on this computer rather than stored on the device.
    pub fn is_software(&self) -> bool {
        matches!(
            self,
            Self::Text { .. } | Self::Open { .. } | Self::Shell { .. } | Self::Macro { .. }
        )
    }

    /// The `type` tag, as [`crate::Capabilities::actions`] lists it.
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Keys { .. } => "keys",
            Self::Media { .. } => "media",
            Self::Mouse { .. } => "mouse",
            Self::Raw { .. } => "raw",
            Self::Text { .. } => "text",
            Self::Open { .. } => "open",
            Self::Shell { .. } => "shell",
            Self::Macro { .. } => "macro",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MediaRef {
    Code(u32),
    Name(String),
}

impl MediaRef {
    /// The HID Consumer usage.
    pub fn usage(&self) -> Result<u16> {
        let raw = |n: u32| -> Result<u16> {
            u16::try_from(n)
                .ok()
                .filter(|&c| c != 0)
                .ok_or_else(|| anyhow!("media code must be between 1 and 0xFFFF"))
        };
        match self {
            Self::Code(n) => raw(*n),
            Self::Name(s) => {
                let s = s.trim().to_lowercase();
                if let Some(code) = crate::keys::media_code(&s) {
                    return Ok(code);
                }
                let n = match s.strip_prefix("0x") {
                    Some(hex) => u32::from_str_radix(hex, 16),
                    None => s.parse(),
                };
                raw(n.map_err(|_| anyhow!("unknown media key {s:?}"))?)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MouseKind {
    Click,
    Move,
    Drag,
    Wheel,
}

fn one() -> u32 {
    1
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "do", rename_all = "lowercase")]
pub enum MacroStep {
    Keys {
        #[serde(default)]
        value: String,
        #[serde(default = "one")]
        repeat: u32,
    },
    Text {
        #[serde(default)]
        value: String,
        #[serde(default = "one")]
        repeat: u32,
    },
    Wait {
        #[serde(default)]
        ms: u32,
    },
    Open {
        #[serde(default)]
        value: String,
        #[serde(default = "one")]
        repeat: u32,
    },
    Shell {
        #[serde(default)]
        value: String,
        #[serde(default = "one")]
        repeat: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KnobAction {
    Ccw,
    Press,
    Cw,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    Button(u8),
    Knob(u8, KnobAction),
}

impl Slot {
    pub fn parse(name: &str) -> Result<Self> {
        let bad = || anyhow!("unknown slot {name:?}");
        if let Some(n) = name.strip_prefix('b') {
            return Ok(Self::Button(n.parse().map_err(|_| bad())?));
        }
        if let Some((k, a)) = name.strip_prefix('k').and_then(|r| r.split_once('.')) {
            let action = match a {
                "ccw" => KnobAction::Ccw,
                "press" => KnobAction::Press,
                "cw" => KnobAction::Cw,
                _ => bail!(bad()),
            };
            return Ok(Self::Knob(k.parse().map_err(|_| bad())?, action));
        }
        Err(bad())
    }
}

impl Mapping {
    pub fn buttons(&self) -> u8 {
        self.rows.saturating_mul(self.columns)
    }

    /// Buttons row by row, then each knob's ccw, press, cw.
    pub fn slot_names(&self) -> Vec<String> {
        let mut names: Vec<String> = (0..self.buttons()).map(|i| format!("b{i}")).collect();
        for k in 0..self.knobs {
            names.extend(KNOB_ACTIONS.iter().map(|a| format!("k{k}.{a}")));
        }
        names
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_macropad_studio_files_as_ch57x() {
        let m: Mapping = serde_json::from_str(
            r#"{"rows": 1, "columns": 2, "knobs": 0, "layers": [{"slots": {
                "b0": {"type": "media", "code": 438, "label": "Mic"}, "b1": null}}]}"#,
        )
        .unwrap();
        assert_eq!((m.driver.as_str(), m.model.as_str()), ("ch57x", "ch57x-1"));
        assert!(!m.layers[0].slots.contains_key("b1"));
        let back: Mapping = serde_json::from_str(&serde_json::to_string(&m).unwrap()).unwrap();
        assert_eq!(back, m);
    }

    #[test]
    fn media_usages() {
        assert_eq!(MediaRef::Name("volumeup".into()).usage().unwrap(), 0xE9);
        assert_eq!(MediaRef::Name("0x1B6".into()).usage().unwrap(), 0x1B6);
        assert_eq!(MediaRef::Code(438).usage().unwrap(), 0x1B6);
        assert!(MediaRef::Code(0).usage().is_err());
        assert!(MediaRef::Name("loud".into()).usage().is_err());
    }

    #[test]
    fn slots() {
        assert_eq!(Slot::parse("b11").unwrap(), Slot::Button(11));
        assert_eq!(Slot::parse("k1.cw").unwrap(), Slot::Knob(1, KnobAction::Cw));
        assert!(Slot::parse("k1.left").is_err());
    }
}
