//! Programmer engine for CH57x macro keypads: the protocol encoders and
//! YAML config of kriomant/ch57x-keyboard-tool, plus a hidapi transport.
//!
//! This crate knows nothing about OpenKeyForge's keymap model; the CH57x
//! driver (crates/drivers/ch57x) translates keymaps into the types here.

pub mod config;
pub mod device;
// Vendored from ch57x-keyboard-tool and kept as close to upstream as possible,
// so upstream's style is left alone rather than changed for newer lints.
#[allow(clippy::new_without_default, clippy::collapsible_if, clippy::manual_is_multiple_of)]
pub mod keyboard;
pub mod parse;

pub use config::KeyboardModel;

use anyhow::Result;
use keyboard::{k884x, k8850_4x4, k8890, Keyboard};

/// The protocol encoder for a model. `buttons`/`knobs` only matter to
/// ch57x-1, whose key ids depend on the layout.
pub fn create_driver(model: KeyboardModel, buttons: u8, knobs: u8) -> Result<Box<dyn Keyboard>> {
    let keyboard: Box<dyn Keyboard> = match model {
        KeyboardModel::Ch57x_1 => Box::new(k884x::Keyboard884x::new(buttons, knobs)?),
        KeyboardModel::Ch57x_2 => Box::new(k8890::Keyboard8890::new()),
        KeyboardModel::Ch57x_3 => Box::new(k8850_4x4::Keyboard8850_4x4::new()),
    };
    Ok(keyboard)
}

/// One line per 64-byte packet, trailing zeros dropped, as `--dry-run` prints them.
pub fn hex_lines(packets: &[u8]) -> Vec<String> {
    packets
        .chunks(64)
        .map(|p| {
            let end = p.iter().rposition(|&b| b != 0).map_or(1, |i| i + 1);
            p[..end].iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ")
        })
        .collect()
}
