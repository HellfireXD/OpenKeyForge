//! OpenKeyForge driver for CH57x macro keypads (1189:8840 and relatives).
//!
//! Translates the standard keymap into the ch57x engine's protocol types
//! (crates/engines/ch57x) and has the engine write them over hidapi. Keys
//! are written in the shared [`okf_core::keys`] vocabulary and converted to
//! HID codes here, so the bytes match the engine's own parser exactly.
//!
//! The keypad can't report its keymap, so every write rewrites every slot:
//! an empty slot is cleared rather than left holding whatever was there.

pub mod import;

use std::collections::BTreeMap;
use std::str::FromStr;

use anyhow::{Context as _, Result, anyhow, bail, ensure};
use ch57x::KeyboardModel;
use ch57x::keyboard::k884x::LedMode;
use ch57x::keyboard::{
    Accord, Code, Key, KeyboardEvent, KnobAction, Macro, MacroOptions, MediaKey, Modifiers,
    MouseAction, MouseButton, MouseButtons, MouseEvent, MouseModifier,
};
use okf_core::agent::{check_not_trigger, is_trigger_chord};
use okf_core::driver::{
    Capabilities, DetectedDevice, DeviceDriver, Encoded, LayoutLimits, LedCaps, ModelInfo,
};
use okf_core::hidapi::HidApi;
use okf_core::keys;
use okf_core::model::{self, Action, ActionKind, Layer, Mapping, MediaRef, MouseKind, Slot};

pub const DRIVER_ID: &str = "ch57x";

const LED_MODES: [&str; 5] = ["off", "backlight", "shock", "shock2", "press"];
const LED_COLORS: [&str; 8] = [
    "white", "red", "orange", "yellow", "green", "cyan", "blue", "purple",
];

pub struct Ch57xDriver;

fn model_id(m: KeyboardModel) -> &'static str {
    match m {
        KeyboardModel::Ch57x_1 => "ch57x-1",
        KeyboardModel::Ch57x_2 => "ch57x-2",
        KeyboardModel::Ch57x_3 => "ch57x-3",
    }
}

fn engine_model(id: &str) -> Result<KeyboardModel> {
    KeyboardModel::from_str(id).map_err(|_| anyhow!("unknown CH57x model {id:?}"))
}

fn capabilities(model: KeyboardModel) -> Capabilities {
    let base = Capabilities {
        actions: vec!["keys", "media", "mouse", "raw"],
        max_layers: 3,
        max_steps: 18,
        max_delay: 6000,
        layout: LayoutLimits {
            rows: (1, 15),
            columns: (1, 15),
            knobs: (0, 4),
            default: (3, 4, 2),
        },
        led: Some(LedCaps {
            modes: LED_MODES.to_vec(),
            colors: LED_COLORS.to_vec(),
        }),
        read_keymap: false,
        packet_preview: true,
    };
    match model {
        KeyboardModel::Ch57x_1 => base,
        // k8890: up to 12 buttons, 3 knobs, 5-step sequences, no delay; its
        // LED protocol takes a bare mode number nobody has mapped yet.
        KeyboardModel::Ch57x_2 => Capabilities {
            max_steps: 5,
            max_delay: 0,
            layout: LayoutLimits {
                rows: (1, 12),
                columns: (1, 12),
                knobs: (0, 3),
                default: (1, 3, 1),
            },
            led: None,
            ..base
        },
        // k8850 4x4: up to 16 buttons, no delay, no drag, no known LED protocol.
        KeyboardModel::Ch57x_3 => Capabilities {
            max_delay: 0,
            layout: LayoutLimits {
                rows: (1, 16),
                columns: (1, 16),
                knobs: (0, 3),
                default: (4, 4, 0),
            },
            led: None,
            ..base
        },
    }
}

/// Clears a slot: a keyboard macro with no keys.
fn noop() -> Macro {
    Macro::Keyboard(KeyboardEvent(
        MacroOptions::default(),
        vec![Accord::new(Modifiers::empty(), None)],
    ))
}

fn engine_key(slot: Slot) -> Key {
    match slot {
        Slot::Button(n) => Key::Button(n),
        Slot::Knob(n, a) => Key::Knob(
            n,
            match a {
                model::KnobAction::Ccw => KnobAction::RotateCCW,
                model::KnobAction::Press => KnobAction::Press,
                model::KnobAction::Cw => KnobAction::RotateCW,
            },
        ),
    }
}

fn engine_accord(a: keys::Accord) -> Accord {
    // Every modifier bit maps to an engine modifier (same HID bit order), and
    // a custom code encodes to the same byte as the named key.
    Accord::new(Modifiers::from_u8(a.mods), a.code.map(Code::Custom))
}

fn delta(v: i32, what: &str) -> Result<i8> {
    i8::try_from(v).map_err(|_| anyhow!("{what} must be between -128 and 127"))
}

/// The engine macro for a hardware action. `user` actions may not use a
/// trigger chord; the chords [`okf_core::agent::lower`] inserts may.
fn engine_macro(kind: &ActionKind, user: bool) -> Result<Macro> {
    let m = match kind {
        ActionKind::Keys { steps, delay } => {
            let steps: Vec<&str> = steps
                .iter()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect();
            ensure!(!steps.is_empty(), "add at least one key");
            ensure!(*delay <= 6000, "delay must be between 0 and 6000 ms");
            let mut accords = Vec::with_capacity(steps.len());
            for s in steps {
                let a = keys::parse_accord(s)?;
                if user {
                    check_not_trigger(&a)?;
                }
                accords.push(engine_accord(a));
            }
            Macro::Keyboard(KeyboardEvent(
                MacroOptions {
                    delay: *delay as u16,
                },
                accords,
            ))
        }
        ActionKind::Media { code } => Macro::Media(MediaKey::Raw(code.usage()?)),
        ActionKind::Mouse {
            action,
            buttons,
            modifier,
            dx,
            dy,
            delta: wheel,
        } => {
            let modifier = match modifier.trim() {
                "" => None,
                m => Some(
                    MouseModifier::from_str(m)
                        .map_err(|_| anyhow!("mouse modifier must be ctrl, shift or alt"))?,
                ),
            };
            let buttons = buttons
                .iter()
                .map(|b| {
                    MouseButton::from_str(b)
                        .map_err(|_| anyhow!("mouse buttons must be left, right or middle"))
                })
                .collect::<Result<MouseButtons>>()?;
            let action = match action {
                MouseKind::Click => {
                    ensure!(!buttons.is_empty(), "pick at least one mouse button");
                    MouseAction::Click(buttons)
                }
                MouseKind::Wheel => MouseAction::Wheel(delta(*wheel, "scroll amount")?),
                MouseKind::Move => MouseAction::Move(delta(*dx, "X")?, delta(*dy, "Y")?),
                MouseKind::Drag => MouseAction::Drag(buttons, delta(*dx, "X")?, delta(*dy, "Y")?),
            };
            Macro::Mouse(MouseEvent(action, modifier))
        }
        ActionKind::Raw { expr } => {
            let e = expr.trim().to_lowercase();
            ensure!(!e.is_empty(), "write an expression");
            ensure!(e.chars().count() <= 500, "expression is too long");
            let m = Macro::from_str(&e).map_err(|_| anyhow!("can't read expression {e:?}"))?;
            if let (true, Macro::Keyboard(KeyboardEvent(_, accords))) = (user, &m) {
                for a in accords {
                    if let Some(code) = a.code {
                        ensure!(
                            !is_trigger_chord(a.modifiers.as_u8(), code.value()),
                            "this shortcut is reserved for agent triggers"
                        );
                    }
                }
            }
            m
        }
        _ => bail!("not a hardware action"),
    };
    Ok(m)
}

impl Ch57xDriver {
    fn engine(&self, m: &Mapping) -> Result<Box<dyn ch57x::keyboard::Keyboard>> {
        ch57x::create_driver(engine_model(&m.model)?, m.buttons(), m.knobs)
    }
}

impl DeviceDriver for Ch57xDriver {
    fn id(&self) -> &'static str {
        DRIVER_ID
    }

    fn name(&self) -> &'static str {
        "CH57x macro keypads"
    }

    fn models(&self) -> Vec<ModelInfo> {
        [
            (
                KeyboardModel::Ch57x_1,
                "ch57x-1 (1189:8840, 8842, 8850; 514c:8851)",
            ),
            (KeyboardModel::Ch57x_2, "ch57x-2 (1189:8890)"),
            (KeyboardModel::Ch57x_3, "ch57x-3 (514c:8850, 4×4)"),
        ]
        .into_iter()
        .map(|(m, name)| ModelInfo {
            id: model_id(m),
            name,
            capabilities: capabilities(m),
        })
        .collect()
    }

    /// Copy/paste/undo, media keys, and volume and scroll on the knobs for
    /// ch57x-1; an empty keymap of the usual size for the others. Shortcuts
    /// use ⌘ on macOS and Ctrl elsewhere.
    fn starter(&self, model: &str) -> Result<Mapping> {
        let caps = capabilities(engine_model(model)?);
        let (rows, columns, knobs) = caps.layout.default;
        let blank = |led: Option<&str>| Layer {
            name: None,
            led: led.map(str::to_string),
            slots: BTreeMap::new(),
        };
        let mut m = Mapping {
            driver: DRIVER_ID.into(),
            model: model.into(),
            rows,
            columns,
            knobs,
            layers: vec![blank(None), blank(None), blank(None)],
        };
        if model != "ch57x-1" {
            return Ok(m);
        }

        let mk = if cfg!(target_os = "macos") {
            "cmd"
        } else {
            "ctrl"
        };
        let keys = |s: &str| {
            Action::new(ActionKind::Keys {
                steps: vec![s.replace('M', mk)],
                delay: 0,
            })
        };
        let media = |s: &str| {
            Action::new(ActionKind::Media {
                code: MediaRef::Name(s.into()),
            })
        };
        let mouse = |action, buttons: &[&str], delta| {
            Action::new(ActionKind::Mouse {
                action,
                buttons: buttons.iter().map(|s| s.to_string()).collect(),
                modifier: String::new(),
                dx: 0,
                dy: 0,
                delta,
            })
        };
        m.layers[0].slots = [
            ("b0", keys("M-c")),
            ("b1", keys("M-v")),
            ("b2", keys("M-z")),
            ("b3", keys("M-shift-z")),
            ("b4", keys("M-x")),
            ("b5", keys("M-a")),
            ("b6", keys("M-s")),
            ("b7", keys("M-f")),
            ("b8", media("previous")),
            ("b9", media("play")),
            ("b10", media("next")),
            ("b11", media("mute")),
            ("k0.ccw", media("volumedown")),
            ("k0.press", media("mute")),
            ("k0.cw", media("volumeup")),
            ("k1.ccw", mouse(MouseKind::Wheel, &[], 1)),
            ("k1.press", mouse(MouseKind::Click, &["middle"], 0)),
            ("k1.cw", mouse(MouseKind::Wheel, &[], -1)),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect();
        for (layer, led) in
            m.layers
                .iter_mut()
                .zip(["backlight white", "backlight blue", "backlight purple"])
        {
            layer.led = Some(led.into());
        }
        Ok(m)
    }

    fn detect(&self, hid: &HidApi) -> Vec<DetectedDevice> {
        ch57x::device::list(hid)
            .into_iter()
            .map(|k| DetectedDevice {
                driver: DRIVER_ID,
                name: if k.product.is_empty() {
                    "CH57x keypad".into()
                } else {
                    k.product.clone()
                },
                path: k.path,
                vendor_id: k.vendor_id,
                product_id: k.product_id,
                models: k.models.into_iter().map(model_id).collect(),
            })
            .collect()
    }

    fn input_interfaces(&self, hid: &HidApi) -> Vec<String> {
        ch57x::device::input_interfaces(hid)
            .into_iter()
            .map(|d| d.path().to_string_lossy().into_owned())
            .collect()
    }

    fn check_layout(&self, m: &Mapping) -> Result<()> {
        let caps = self.capabilities(&m.model)?;
        let l = &caps.layout;
        let within = |v: u8, (lo, hi): (u8, u8), what: &str| -> Result<()> {
            ensure!(
                (lo..=hi).contains(&v),
                "{what} must be between {lo} and {hi}"
            );
            Ok(())
        };
        within(m.rows, l.rows, "rows")?;
        within(m.columns, l.columns, "columns")?;
        within(m.knobs, l.knobs, "knobs")?;
        ensure!(
            (1..=caps.max_layers).contains(&m.layers.len()),
            "the keypad has 1-{} layers",
            caps.max_layers
        );

        // Every slot must have a key id on this model; a scratch bind finds out.
        let engine = self.engine(m)?;
        let mut scratch = Vec::new();
        for name in m.slot_names() {
            engine
                .bind_key(0, engine_key(Slot::parse(&name)?), &noop(), &mut scratch)
                .map_err(|e| anyhow!("{} has no {name} slot: {e}", m.model))?;
        }

        for (i, layer) in m.layers.iter().enumerate() {
            if let Some(name) = &layer.name {
                ensure!(name.chars().count() <= 60, "layer name is too long");
            }
            if let Some(led) = &layer.led {
                ensure!(
                    caps.led.is_some() || led == "off",
                    "layer {}: LED modes are only known for ch57x-1",
                    i + 1
                );
                LedMode::from_str(led)
                    .map_err(|_| anyhow!("layer {}: unknown LED mode {led:?}", i + 1))?;
            }
            for action in layer.slots.values() {
                if let Some(label) = &action.label {
                    ensure!(label.chars().count() <= 60, "label is too long");
                }
            }
        }
        Ok(())
    }

    fn check_action(
        &self,
        m: &Mapping,
        layer: usize,
        slot: Slot,
        action: &ActionKind,
    ) -> Result<()> {
        // The engine has model-specific limits (no delay on ch57x-2, no drag
        // on ch57x-3, …), so bind into a scratch buffer.
        let macro_ = engine_macro(action, true)?;
        self.engine(m)?
            .bind_key(layer as u8, engine_key(slot), &macro_, &mut Vec::new())
    }

    fn encode(&self, m: &Mapping) -> Result<Encoded> {
        let mut engine = self.engine(m)?;
        let mut packets = Vec::new();
        for (li, layer) in m.layers.iter().enumerate() {
            for name in m.slot_names() {
                let macro_ = match layer.slots.get(&name) {
                    None => noop(),
                    Some(action) => engine_macro(&action.kind, false)
                        .with_context(|| format!("layer {} {name}", li + 1))?,
                };
                engine
                    .bind_key(
                        li as u8,
                        engine_key(Slot::parse(&name)?),
                        &macro_,
                        &mut packets,
                    )
                    .with_context(|| format!("layer {} {name}", li + 1))?;
            }
            if let Some(led) = &layer.led {
                engine
                    .set_led(&["led".into(), li.to_string(), led.clone()], &mut packets)
                    .with_context(|| format!("layer {} LED", li + 1))?;
            }
        }
        Ok(Encoded {
            preview: okf_core::hex_lines(&packets),
            payload: packets,
        })
    }

    fn write(&self, hid: &HidApi, device: &DetectedDevice, encoded: &Encoded) -> Result<()> {
        ch57x::device::send(hid, &device.path, &encoded.payload)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use okf_core::driver::{prepare, slot_errors};

    fn mapping(json: &str) -> Mapping {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn starter_prepares() {
        let p = prepare(&Ch57xDriver, &Ch57xDriver.starter("ch57x-1").unwrap()).unwrap();
        assert_eq!(p.hardware, 18);
        assert!(p.triggers.is_empty());
        // 3 layers × 18 slots × (bind + 3 commit packets) + 3 × 2 LED packets.
        assert_eq!(p.encoded.preview.len(), 3 * 18 * 4 + 3 * 2);
        for model in ["ch57x-2", "ch57x-3"] {
            prepare(&Ch57xDriver, &Ch57xDriver.starter(model).unwrap()).unwrap();
        }
    }

    /// The bytes macropad-studio's test suite checks for a keystroke slot.
    #[test]
    fn keys_bytes_match_studio() {
        let m = mapping(
            r#"{"rows": 1, "columns": 1, "knobs": 0, "layers": [
            {"slots": {"b0": {"type": "keys", "steps": ["ctrl-a"]}}}]}"#,
        );
        let p = prepare(&Ch57xDriver, &m).unwrap();
        assert_eq!(
            p.encoded.preview,
            [
                "03 fe 01 01 01 00 00 00 00 00 01 01 04",
                "03 aa aa",
                "03 fd fe ff",
                "03 aa aa"
            ]
        );
    }

    /// The driver's key vocabulary must encode exactly as the engine's own parser.
    #[test]
    fn every_key_encodes_like_the_engine_parser() {
        let engine = ch57x::create_driver(KeyboardModel::Ch57x_1, 1, 0).unwrap();
        for name in keys::KEY_NAMES {
            let step = format!("ctrl-shift-{name}");
            let ours = engine_macro(
                &ActionKind::Keys {
                    steps: vec![step.clone()],
                    delay: 0,
                },
                false,
            )
            .unwrap();
            let theirs = Macro::from_str(&step).unwrap();
            let (mut a, mut b) = (Vec::new(), Vec::new());
            engine.bind_key(0, Key::Button(0), &ours, &mut a).unwrap();
            engine.bind_key(0, Key::Button(0), &theirs, &mut b).unwrap();
            assert_eq!(a, b, "{step}");
        }
    }

    #[test]
    fn raw_consumer_code() {
        let m = mapping(
            r#"{"rows": 1, "columns": 1, "knobs": 0, "layers": [
            {"slots": {"b0": {"type": "media", "code": "0x1b6"}}}]}"#,
        );
        assert_eq!(
            prepare(&Ch57xDriver, &m).unwrap().encoded.preview[0],
            "03 fe 01 01 02 00 00 00 00 00 00 b6 01"
        );
    }

    #[test]
    fn software_slots_become_trigger_chords() {
        let m = mapping(
            r#"{"rows": 1, "columns": 2, "knobs": 0, "layers": [{"slots": {
            "b0": {"type": "text", "text": "hi"}, "b1": {"type": "media", "code": "mute"}}}]}"#,
        );
        let p = prepare(&Ch57xDriver, &m).unwrap();
        assert_eq!((p.hardware, p.triggers.len()), (1, 1));
        // ctrl+alt+shift (0x07) + F16 (0x6b) stored in b0.
        assert_eq!(
            p.encoded.preview[0],
            "03 fe 01 01 01 00 00 00 00 00 01 07 6b"
        );
    }

    #[test]
    fn errors_are_per_slot() {
        let m = mapping(
            r#"{"rows": 1, "columns": 3, "knobs": 0, "layers": [{"slots": {
            "b0": {"type": "keys", "steps": ["nosuchkey"]},
            "b1": {"type": "raw", "expr": "ctrl-alt-shift-f16"},
            "b2": {"type": "open", "target": "-rf"}}}]}"#,
        );
        let errs = slot_errors(&Ch57xDriver, &m).unwrap();
        assert!(errs["0:b0"].contains("unknown key"));
        assert!(errs["0:b1"].contains("reserved"));
        assert!(errs["0:b2"].contains("'-'"));
        assert!(prepare(&Ch57xDriver, &m).is_err());
    }

    #[test]
    fn model_limits_surface_per_slot() {
        let m = mapping(
            r#"{"model": "ch57x-2", "rows": 1, "columns": 3, "knobs": 1, "layers": [{"slots": {
            "b0": {"type": "keys", "steps": ["a", "b"], "delay": 100}}}]}"#,
        );
        assert!(slot_errors(&Ch57xDriver, &m).unwrap()["0:b0"].contains("delay"));
    }

    #[test]
    fn led_only_on_ch57x_1() {
        let m = mapping(
            r#"{"model": "ch57x-3", "rows": 4, "columns": 4, "knobs": 0,
            "layers": [{"led": "backlight red", "slots": {}}]}"#,
        );
        assert!(Ch57xDriver.check_layout(&m).is_err());
    }
}
