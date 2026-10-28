//! Actions the hardware can't store: typing text, opening things, shell
//! commands and macros.
//!
//! Before a mapping reaches a driver, [`lower`] swaps each software action
//! for a hidden *trigger chord* (3–4 modifiers plus F13–F20) as a plain
//! `keys` action, which every device can store. The app's agent catches the
//! chord and performs the real action, so drivers never see software actions.
//! F21–F24 are left out because macOS has no keycodes for them.

use std::collections::HashSet;

use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

use crate::keys::{Accord, fold_mods, parse_accord};
use crate::model::{Action, ActionKind, MacroStep, Mapping};

/// F16–F20, F13, then F14/F15, which are screen brightness on some Macs.
pub const TRIGGER_KEYS: [u8; 8] = [0x6B, 0x6C, 0x6D, 0x6E, 0x6F, 0x68, 0x69, 0x6A];

/// HID modifier bits (ctrl 1, shift 2, alt 4, win/cmd 8). Ctrl+Alt+Shift+Win
/// comes last because Windows reserves it for the Office key, so Windows
/// stops one set short.
pub const TRIGGER_MODS: [u8; 5] = [0x07, 0x0D, 0x0B, 0x0E, 0x0F];

pub const MAX_MACRO_STEPS: usize = 500;

pub const fn max_software_slots() -> usize {
    if cfg!(windows) {
        TRIGGER_KEYS.len() * (TRIGGER_MODS.len() - 1)
    } else {
        TRIGGER_KEYS.len() * TRIGGER_MODS.len()
    }
}

/// The chord for the `i`-th software slot.
pub fn trigger(i: usize) -> Result<Accord> {
    ensure!(
        i < max_software_slots(),
        "too many agent actions (max {})",
        max_software_slots()
    );
    Ok(Accord {
        mods: TRIGGER_MODS[i / TRIGGER_KEYS.len()],
        code: Some(TRIGGER_KEYS[i % TRIGGER_KEYS.len()]),
    })
}

pub fn is_trigger_chord(mods: u8, code: u8) -> bool {
    TRIGGER_MODS.contains(&fold_mods(mods)) && TRIGGER_KEYS.contains(&code)
}

/// Rejects hardware key steps that would collide with a trigger chord.
pub fn check_not_trigger(accord: &Accord) -> Result<()> {
    if let Some(code) = accord.code {
        ensure!(
            !is_trigger_chord(accord.mods, code),
            "this shortcut is reserved for agent triggers"
        );
    }
    Ok(())
}

/// A software slot as uploaded: the chord the device sends and what to do.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Trigger {
    pub mods: u8,
    /// HID usage of the key (F13..F20).
    pub code: u8,
    pub layer: usize,
    pub slot: String,
    pub action: Action,
}

fn clip(s: &str, limit: usize, what: &str) -> Result<()> {
    ensure!(s.chars().count() <= limit, "{what} is too long");
    Ok(())
}

fn check_open_target(target: &str) -> Result<()> {
    let t = target.trim();
    ensure!(!t.is_empty(), "choose something to open");
    // Openers read a leading "-" as one of their own options.
    ensure!(!t.starts_with('-'), "an open target can't start with '-'");
    clip(t, 2000, "target")
}

fn check_repeat(repeat: u32) -> Result<()> {
    ensure!(
        (1..=999).contains(&repeat),
        "repeat must be between 1 and 999"
    );
    Ok(())
}

/// Raises if the agent couldn't perform this software action.
pub fn check_software(kind: &ActionKind) -> Result<()> {
    match kind {
        ActionKind::Text { text } => clip(text, 10_000, "text"),
        ActionKind::Open { target } => check_open_target(target),
        ActionKind::Shell { command } => {
            ensure!(!command.trim().is_empty(), "write a command");
            clip(command, 10_000, "command")
        }
        ActionKind::Macro { steps } => {
            ensure!(!steps.is_empty(), "add at least one step");
            ensure!(
                steps.len() <= MAX_MACRO_STEPS,
                "a macro has at most {MAX_MACRO_STEPS} steps"
            );
            for step in steps {
                match step {
                    MacroStep::Keys { value, repeat } => {
                        check_repeat(*repeat)?;
                        let accords: Vec<&str> = value
                            .split(',')
                            .map(str::trim)
                            .filter(|s| !s.is_empty())
                            .collect();
                        ensure!(!accords.is_empty(), "a key step needs at least one key");
                        for a in accords {
                            parse_accord(a)?;
                        }
                    }
                    MacroStep::Text { value, repeat } => {
                        check_repeat(*repeat)?;
                        clip(value, 10_000, "text")?;
                    }
                    MacroStep::Wait { ms } => {
                        ensure!(*ms <= 600_000, "a wait is at most 600000 ms")
                    }
                    MacroStep::Open { value, repeat } => {
                        check_repeat(*repeat)?;
                        check_open_target(value)?;
                    }
                    MacroStep::Shell { value, repeat } => {
                        check_repeat(*repeat)?;
                        ensure!(!value.trim().is_empty(), "a shell step needs a command");
                    }
                }
            }
            Ok(())
        }
        _ => Err(anyhow::anyhow!("not an agent action")),
    }
}

/// Replaces every software action with its trigger chord. Returns the
/// hardware-only mapping a driver can write, and the triggers the agent
/// listens for.
pub fn lower(m: &Mapping) -> Result<(Mapping, Vec<Trigger>)> {
    lower_avoiding(m, &HashSet::new())
}

/// Like [`lower`], but never assigns a chord in `taken`: chords live on
/// other attached devices, so two devices never send the same chord for
/// different actions.
pub fn lower_avoiding(m: &Mapping, taken: &HashSet<(u8, u8)>) -> Result<(Mapping, Vec<Trigger>)> {
    let mut free = (0..max_software_slots())
        .map(trigger)
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .filter(|a| !taken.contains(&(a.mods, a.code.unwrap_or(0))));
    let mut out = m.clone();
    let mut triggers = Vec::new();
    for (li, layer) in out.layers.iter_mut().enumerate() {
        for name in m.slot_names() {
            let Some(action) = layer.slots.get_mut(&name) else {
                continue;
            };
            if !action.kind.is_software() {
                continue;
            }
            check_software(&action.kind)?;
            let chord = free.next().ok_or_else(|| {
                anyhow::anyhow!(
                    "out of agent chords: all {} are used across your attached devices",
                    max_software_slots()
                )
            })?;
            triggers.push(Trigger {
                mods: chord.mods,
                code: chord.code.expect("trigger chords have a key"),
                layer: li,
                slot: name.clone(),
                action: action.clone(),
            });
            *action = Action::new(ActionKind::Keys {
                steps: vec![chord.to_expr()],
                delay: 0,
            });
        }
    }
    Ok((out, triggers))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn triggers_never_repeat() {
        let all: std::collections::HashSet<(u8, u8)> = (0..max_software_slots())
            .map(|i| trigger(i).unwrap())
            .map(|a| (a.mods, a.code.unwrap()))
            .collect();
        assert_eq!(all.len(), max_software_slots());
        assert!(trigger(max_software_slots()).is_err());
        assert!(all.iter().all(|&(m, c)| is_trigger_chord(m, c)));
    }

    #[test]
    fn lowering_skips_chords_taken_by_other_devices() {
        let m: Mapping = serde_json::from_str(
            r#"{"rows": 1, "columns": 1, "knobs": 0, "layers": [{"slots": {
                "b0": {"type": "text", "text": "hi"}}}]}"#,
        )
        .unwrap();
        let first = trigger(0).unwrap();
        let taken = HashSet::from([(first.mods, first.code.unwrap())]);
        let (_, triggers) = lower_avoiding(&m, &taken).unwrap();
        let second = trigger(1).unwrap();
        assert_eq!(
            (triggers[0].mods, triggers[0].code),
            (second.mods, second.code.unwrap())
        );
    }

    #[test]
    fn lowering_swaps_software_actions_for_chords() {
        let m: Mapping = serde_json::from_str(
            r#"{"rows": 1, "columns": 3, "knobs": 0, "layers": [{"slots": {
                "b0": {"type": "keys", "steps": ["a"]},
                "b1": {"type": "text", "text": "hi"},
                "b2": {"type": "shell", "command": "echo hi"}}}]}"#,
        )
        .unwrap();
        let (hw, triggers) = lower(&m).unwrap();
        assert_eq!(triggers.len(), 2);
        assert_eq!(
            (triggers[0].slot.as_str(), triggers[1].slot.as_str()),
            ("b1", "b2")
        );
        assert_eq!(hw.layers[0].slots["b0"], m.layers[0].slots["b0"]);
        let ActionKind::Keys { steps, .. } = &hw.layers[0].slots["b1"].kind else {
            panic!()
        };
        assert_eq!(steps, &["ctrl-shift-alt-f16"]);
        assert!(hw.layers[0].slots.values().all(|a| !a.kind.is_software()));
    }
}
