//! Turns what the keypad sends into press events, for the live key test and
//! the Linux agent.
//!
//! macropad-studio read these through IOKit, which hands over decoded
//! (usage page, usage, value) triples. hidapi hands over raw reports, so this
//! module parses the interface's report descriptor and decodes reports the
//! same way, on every platform.

use std::collections::{HashMap, HashSet};

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum KeypadEvent {
    /// HID Consumer usage (volume, play, …).
    Media {
        code: u32,
    },
    Wheel {
        delta: i32,
    },
    Move {
        axis: char,
        delta: i32,
    },
    Button {
        button: &'static str,
    },
    /// Keyboard usage with the modifier bits held in the same report.
    Key {
        mods: u8,
        code: u8,
    },
}

#[derive(Debug, Clone)]
struct Field {
    report_id: u8,
    bit_offset: usize,
    size: usize,
    count: usize,
    page: u16,
    usages: Vec<u32>,
    usage_min: u32,
    usage_max: u32,
    logical_min: i32,
    logical_max: i32,
    variable: bool,
    constant: bool,
}

impl Field {
    /// The usage for the `i`-th element of a variable field, or the `i`-th
    /// array index; usages can carry their own page in the high 16 bits.
    fn usage(&self, i: usize) -> Option<(u16, u32)> {
        let u = if !self.usages.is_empty() {
            *self.usages.get(i).or(self.usages.last())?
        } else if self.usage_max >= self.usage_min {
            let u = self.usage_min + i as u32;
            if u > self.usage_max {
                return None;
            }
            u
        } else {
            return None;
        };
        Some(if u > 0xFFFF {
            ((u >> 16) as u16, u & 0xFFFF)
        } else {
            (self.page, u)
        })
    }
}

/// The input fields of one interface.
#[derive(Debug, Clone, Default)]
pub struct Descriptor {
    fields: Vec<Field>,
    uses_report_ids: bool,
}

fn signed(v: u32, size: usize) -> i32 {
    match size {
        1 => v as u8 as i8 as i32,
        2 => v as u16 as i16 as i32,
        _ => v as i32,
    }
}

impl Descriptor {
    pub fn parse(desc: &[u8]) -> Self {
        #[derive(Clone, Default)]
        struct Globals {
            page: u16,
            logical_min: i32,
            logical_max: i32,
            size: usize,
            count: usize,
            report_id: u8,
        }
        let mut g = Globals::default();
        let mut stack: Vec<Globals> = Vec::new();
        let (mut usages, mut umin, mut umax) = (Vec::new(), 0u32, 0u32);
        let mut offsets: HashMap<u8, usize> = HashMap::new();
        let mut out = Descriptor::default();

        let mut i = 0;
        while i < desc.len() {
            let prefix = desc[i];
            if prefix == 0xFE {
                // Long item: skip it.
                let len = *desc.get(i + 1).unwrap_or(&0) as usize;
                i += 3 + len;
                continue;
            }
            let size = [0, 1, 2, 4][(prefix & 3) as usize];
            let Some(data) = desc.get(i + 1..i + 1 + size) else {
                break;
            };
            let raw = data.iter().rev().fold(0u32, |v, &b| (v << 8) | b as u32);
            let (kind, tag) = ((prefix >> 2) & 3, prefix >> 4);
            i += 1 + size;

            match (kind, tag) {
                // Main: Input.
                (0, 8) => {
                    let off = offsets.entry(g.report_id).or_insert(0);
                    out.fields.push(Field {
                        report_id: g.report_id,
                        bit_offset: *off,
                        size: g.size,
                        count: g.count,
                        page: g.page,
                        usages: std::mem::take(&mut usages),
                        usage_min: umin,
                        usage_max: umax,
                        logical_min: g.logical_min,
                        logical_max: g.logical_max,
                        variable: raw & 2 != 0,
                        constant: raw & 1 != 0,
                    });
                    *off += g.size * g.count;
                    (umin, umax) = (0, 0);
                }
                // Other main items reset the locals.
                (0, _) => {
                    usages.clear();
                    (umin, umax) = (0, 0);
                }
                (1, 0) => g.page = raw as u16,
                (1, 1) => g.logical_min = signed(raw, size),
                (1, 2) => {
                    g.logical_max = if g.logical_min < 0 {
                        signed(raw, size)
                    } else {
                        raw as i32
                    }
                }
                (1, 7) => g.size = raw as usize,
                (1, 8) => {
                    g.report_id = raw as u8;
                    out.uses_report_ids = true;
                }
                (1, 9) => g.count = raw as usize,
                (1, 10) => stack.push(g.clone()),
                (1, 11) => g = stack.pop().unwrap_or_default(),
                (2, 0) => usages.push(if size == 4 { raw } else { raw & 0xFFFF }),
                (2, 1) => umin = raw,
                (2, 2) => umax = raw,
                _ => {}
            }
        }
        out
    }

    /// (page, usage, value) for every element in a report; array fields
    /// yield their pressed usages with value 1.
    fn values(&self, report: &[u8]) -> (u8, Vec<(u16, u32, i32)>) {
        let (id, data) = if self.uses_report_ids && !report.is_empty() {
            (report[0], &report[1..])
        } else {
            (0, report)
        };
        let bits = |off: usize, size: usize| -> Option<u32> {
            if size == 0 || size > 32 || (off + size).div_ceil(8) > data.len() {
                return None;
            }
            let mut v = 0u64;
            for b in 0..size {
                let bit = off + b;
                v |= (((data[bit / 8] >> (bit % 8)) & 1) as u64) << b;
            }
            Some(v as u32)
        };
        let mut out = Vec::new();
        for f in self
            .fields
            .iter()
            .filter(|f| f.report_id == id && !f.constant)
        {
            for i in 0..f.count {
                let Some(raw) = bits(f.bit_offset + i * f.size, f.size) else {
                    break;
                };
                let value = if f.logical_min < 0 && f.size < 32 {
                    let shift = 32 - f.size;
                    ((raw << shift) as i32) >> shift
                } else {
                    raw as i32
                };
                if f.variable {
                    if let Some((page, usage)) = f.usage(i) {
                        out.push((page, usage, value));
                    }
                } else if value >= f.logical_min
                    && value <= f.logical_max
                    && let Some((page, usage)) = f.usage((value - f.logical_min) as usize)
                    && usage != 0
                {
                    out.push((page, usage, 1));
                }
            }
        }
        (id, out)
    }
}

/// Decodes one interface's reports into press events. Keyboards and consumer
/// controls repeat held usages in every report, so presses are found by
/// comparing each report with the previous one of the same id.
#[derive(Debug, Default)]
pub struct Decoder {
    desc: Descriptor,
    held: HashMap<u8, HashSet<(u16, u32)>>,
}

impl Decoder {
    pub fn new(descriptor: &[u8]) -> Self {
        Self {
            desc: Descriptor::parse(descriptor),
            held: HashMap::new(),
        }
    }

    pub fn feed(&mut self, report: &[u8]) -> Vec<KeypadEvent> {
        let (id, values) = self.desc.values(report);
        let mods = values
            .iter()
            .filter(|(p, u, v)| *p == 0x07 && (0xE0..=0xE7).contains(u) && *v != 0)
            .fold(0u8, |m, (_, u, _)| m | 1 << (u - 0xE0));
        let now: HashSet<(u16, u32)> = values
            .iter()
            .filter(|(_, _, v)| *v != 0)
            .map(|(p, u, _)| (*p, *u))
            .collect();
        let before = self.held.insert(id, now.clone()).unwrap_or_default();

        let mut events = Vec::new();
        let mut keys = Vec::new();
        for &(page, usage, value) in &values {
            let fresh = value != 0 && !before.contains(&(page, usage));
            match (page, usage) {
                // Relative axes report motion, not state: every non-zero value counts.
                (0x01, 0x38) if value != 0 => events.push(KeypadEvent::Wheel {
                    delta: value.signum(),
                }),
                (0x01, 0x30 | 0x31) if value != 0 => events.push(KeypadEvent::Move {
                    axis: if usage == 0x30 { 'x' } else { 'y' },
                    delta: value,
                }),
                (0x0C, u) if fresh && u != 0 => events.push(KeypadEvent::Media { code: u }),
                (0x09, 1..=3) if fresh => events.push(KeypadEvent::Button {
                    button: ["left", "right", "middle"][usage as usize - 1],
                }),
                (0x07, 0x04..=0xDF) if fresh => keys.push(usage as u8),
                _ => {}
            }
        }
        // Keys are emitted after the whole report, so they carry its modifiers
        // even when the modifier bits come after the key in the report.
        events.extend(keys.into_iter().map(|code| KeypadEvent::Key { mods, code }));
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Composite keyboard (id 1) + consumer (id 2) + mouse (id 3), the shape
    /// of descriptor these keypads use for their input interface.
    const DESC: &[u8] = &[
        0x05, 0x01, 0x09, 0x06, 0xA1, 0x01, 0x85, 0x01, // keyboard, report 1
        0x05, 0x07, 0x19, 0xE0, 0x29, 0xE7, 0x15, 0x00, 0x25, 0x01, 0x75, 0x01, 0x95, 0x08, 0x81,
        0x02, // modifiers
        0x95, 0x01, 0x75, 0x08, 0x81, 0x01, // reserved
        0x95, 0x06, 0x75, 0x08, 0x15, 0x00, 0x26, 0xFF, 0x00, 0x05, 0x07, 0x19, 0x00, 0x2A, 0xFF,
        0x00, 0x81, 0x00, // keys
        0xC0, 0x05, 0x0C, 0x09, 0x01, 0xA1, 0x01, 0x85, 0x02, // consumer, report 2
        0x15, 0x00, 0x26, 0xFF, 0x03, 0x19, 0x00, 0x2A, 0xFF, 0x03, 0x75, 0x10, 0x95, 0x01, 0x81,
        0x00, 0xC0, 0x05, 0x01, 0x09, 0x02, 0xA1, 0x01, 0x85, 0x03, 0x09, 0x01, 0xA1,
        0x00, // mouse, report 3
        0x05, 0x09, 0x19, 0x01, 0x29, 0x03, 0x15, 0x00, 0x25, 0x01, 0x95, 0x03, 0x75, 0x01, 0x81,
        0x02, 0x95, 0x01, 0x75, 0x05, 0x81, 0x01, 0x05, 0x01, 0x09, 0x30, 0x09, 0x31, 0x09, 0x38,
        0x15, 0x81, 0x25, 0x7F, 0x75, 0x08, 0x95, 0x03, 0x81, 0x06, 0xC0, 0xC0,
    ];

    #[test]
    fn keys_carry_their_modifiers_and_fire_once() {
        let mut d = Decoder::new(DESC);
        // ctrl+alt+shift (0x07) + F16 (0x6B), held for two reports.
        let r = [1, 0x07, 0, 0x6B, 0, 0, 0, 0, 0];
        assert_eq!(
            d.feed(&r),
            [KeypadEvent::Key {
                mods: 0x07,
                code: 0x6B
            }]
        );
        assert_eq!(d.feed(&r), []);
        assert_eq!(d.feed(&[1, 0, 0, 0, 0, 0, 0, 0, 0]), []);
        assert_eq!(
            d.feed(&r),
            [KeypadEvent::Key {
                mods: 0x07,
                code: 0x6B
            }]
        );
    }

    #[test]
    fn consumer_and_mouse() {
        let mut d = Decoder::new(DESC);
        assert_eq!(
            d.feed(&[2, 0xE9, 0x00]),
            [KeypadEvent::Media { code: 0xE9 }]
        );
        assert_eq!(d.feed(&[2, 0, 0]), []);
        assert_eq!(
            d.feed(&[3, 0b100, 0, 0, 0xFF]),
            [
                KeypadEvent::Button { button: "middle" },
                KeypadEvent::Wheel { delta: -1 }
            ]
        );
        assert_eq!(
            d.feed(&[3, 0, 5, 0, 0]),
            [KeypadEvent::Move {
                axis: 'x',
                delta: 5
            }]
        );
    }

    #[test]
    fn boot_keyboard_without_report_ids() {
        // The keyboard collection alone, minus its report id item (DESC[6..8]).
        let desc: Vec<u8> = [&DESC[..6], &DESC[8..49]].concat();
        let mut d = Decoder::new(&desc);
        assert_eq!(
            d.feed(&[0x02, 0, 0x04, 0, 0, 0, 0, 0]),
            [KeypadEvent::Key {
                mods: 0x02,
                code: 0x04
            }]
        );
    }
}
