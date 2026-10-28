//! OpenKeyForge's device-independent core.
//!
//! - [`model`]: the standard keymap every driver reads and writes: layers of
//!   button and knob slots, each holding an [`model::Action`].
//! - [`keys`]: the key, modifier and media vocabulary actions are written in.
//! - [`agent`]: actions the hardware can't store, lowered to trigger chords
//!   that the app's agent catches.
//! - [`driver`]: the [`driver::DeviceDriver`] trait each device family
//!   implements, the capabilities it reports, and the [`driver::Registry`].
//! - [`input`]: decodes what a device sends, for the live key test.

pub mod agent;
pub mod driver;
pub mod input;
pub mod keys;
pub mod model;

pub use driver::{Capabilities, DeviceDriver, Registry};
pub use hidapi;
pub use model::{Action, ActionKind, Layer, Mapping};

/// One line per 64-byte packet, trailing zeros dropped, for packet previews.
pub fn hex_lines(packets: &[u8]) -> Vec<String> {
    packets
        .chunks(64)
        .map(|p| {
            let end = p.iter().rposition(|&b| b != 0).map_or(1, |i| i + 1);
            p[..end]
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect()
}
