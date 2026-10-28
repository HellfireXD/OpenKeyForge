//! The middle layer between the editor and the programmer engines.
//!
//! A [`DeviceDriver`] speaks the standard keymap on one side and one device
//! family's engine on the other: it says what the family can do
//! ([`Capabilities`]), finds connected devices, checks a keymap against the
//! hardware, translates it into the engine's own language, and has the
//! engine write it. The app only ever talks to drivers through this trait,
//! via the [`Registry`].
//!
//! Drivers receive hardware-only keymaps: [`prepare`] lowers software
//! actions to trigger chords first (see [`crate::agent`]).

use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::{Result, anyhow, bail};
use hidapi::HidApi;
use serde::Serialize;

use crate::agent::{self, Trigger};
use crate::model::{ActionKind, Mapping, Slot};

/// What a device model supports. The editor shows only what's listed here.
#[derive(Debug, Clone, Serialize)]
pub struct Capabilities {
    /// Hardware action types the device stores (`keys`, `media`, `mouse`, `raw`).
    pub actions: Vec<&'static str>,
    pub max_layers: usize,
    /// Most steps in one `keys` action.
    pub max_steps: usize,
    /// Longest delay between key steps in ms; 0 if the device has none.
    pub max_delay: u32,
    pub layout: LayoutLimits,
    pub led: Option<LedCaps>,
    /// The device can report its keymap ([`DeviceDriver::read_keymap`]).
    pub read_keymap: bool,
    /// The driver can show the raw packets it would send.
    pub packet_preview: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct LayoutLimits {
    pub rows: (u8, u8),
    pub columns: (u8, u8),
    pub knobs: (u8, u8),
    /// Default layout for a new keymap: (rows, columns, knobs).
    pub default: (u8, u8, u8),
}

/// Per-layer lighting, written as `"<mode> <colour>"` or `"off"`.
#[derive(Debug, Clone, Serialize)]
pub struct LedCaps {
    pub modes: Vec<&'static str>,
    pub colors: Vec<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub capabilities: Capabilities,
}

/// A connected device a driver can program.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DetectedDevice {
    pub driver: &'static str,
    /// Transport address (a hidapi path for HID devices).
    pub path: String,
    pub vendor_id: u16,
    pub product_id: u16,
    pub name: String,
    /// Models this device may be. More than one means the user must pick.
    pub models: Vec<&'static str>,
}

impl DetectedDevice {
    pub fn usb_id(&self) -> String {
        format!("{:04x}:{:04x}", self.vendor_id, self.product_id)
    }
}

/// A keymap translated into the engine's language, ready to write.
#[derive(Debug, Clone)]
pub struct Encoded {
    /// Opaque to everything but the driver that made it.
    pub payload: Vec<u8>,
    /// Human-readable form for the packet preview, if the driver has one.
    pub preview: Vec<String>,
}

pub trait DeviceDriver: Send + Sync {
    /// Stable id stored in keymaps, e.g. `"ch57x"`.
    fn id(&self) -> &'static str;

    /// Name for the editor, e.g. `"CH57x macro keypads"`.
    fn name(&self) -> &'static str;

    fn models(&self) -> Vec<ModelInfo>;

    /// The keymap a new user of this model starts with.
    fn starter(&self, model: &str) -> Result<Mapping>;

    /// Connected devices this driver can program.
    fn detect(&self, hid: &HidApi) -> Vec<DetectedDevice>;

    /// hidapi paths of the interfaces the device types into (keyboard,
    /// consumer, mouse), for the live key test and Linux agent chords.
    fn input_interfaces(&self, hid: &HidApi) -> Vec<String>;

    /// Layout, layer count and lighting: everything not about one slot.
    fn check_layout(&self, keymap: &Mapping) -> Result<()>;

    /// One hardware action in one slot. Errors are shown on that slot.
    fn check_action(
        &self,
        keymap: &Mapping,
        layer: usize,
        slot: Slot,
        action: &ActionKind,
    ) -> Result<()>;

    /// Translates a checked, hardware-only keymap for the engine.
    fn encode(&self, keymap: &Mapping) -> Result<Encoded>;

    /// Has the engine write an encoded keymap to `device`.
    fn write(&self, hid: &HidApi, device: &DetectedDevice, encoded: &Encoded) -> Result<()>;

    /// Reads the keymap stored on `device`. Only if
    /// [`Capabilities::read_keymap`] says so.
    fn read_keymap(&self, _hid: &HidApi, _device: &DetectedDevice) -> Result<Mapping> {
        bail!("{} can't report the keymap stored on them", self.name())
    }

    fn capabilities(&self, model: &str) -> Result<Capabilities> {
        self.models()
            .into_iter()
            .find(|m| m.id == model)
            .map(|m| m.capabilities)
            .ok_or_else(|| anyhow!("{} has no model {model:?}", self.name()))
    }
}

/// Every driver the app knows, by id.
#[derive(Default, Clone)]
pub struct Registry {
    drivers: Vec<Arc<dyn DeviceDriver>>,
}

impl Registry {
    pub fn new(drivers: Vec<Arc<dyn DeviceDriver>>) -> Self {
        Self { drivers }
    }

    pub fn get(&self, id: &str) -> Result<Arc<dyn DeviceDriver>> {
        self.drivers
            .iter()
            .find(|d| d.id() == id)
            .cloned()
            .ok_or_else(|| anyhow!("no driver for {id:?} devices"))
    }

    pub fn all(&self) -> &[Arc<dyn DeviceDriver>] {
        &self.drivers
    }

    pub fn detect(&self, hid: &HidApi) -> Vec<DetectedDevice> {
        self.drivers.iter().flat_map(|d| d.detect(hid)).collect()
    }

    pub fn input_interfaces(&self, hid: &HidApi) -> Vec<String> {
        self.drivers
            .iter()
            .flat_map(|d| d.input_interfaces(hid))
            .collect()
    }
}

/// Every slot's problem keyed by `"layer:slot"`, so the editor can mark each
/// bad key. Problems with the keymap as a whole are returned as `Err`.
pub fn slot_errors(driver: &dyn DeviceDriver, m: &Mapping) -> Result<BTreeMap<String, String>> {
    driver.check_layout(m)?;
    let caps = driver.capabilities(&m.model)?;
    let names = m.slot_names();
    for layer in &m.layers {
        if let Some(bad) = layer.slots.keys().find(|s| !names.contains(s)) {
            bail!(
                "unknown slot {bad:?} for a {}×{} keypad with {} knobs",
                m.rows,
                m.columns,
                m.knobs
            );
        }
    }

    let mut errors = BTreeMap::new();
    let mut software = 0;
    for (li, layer) in m.layers.iter().enumerate() {
        for name in &names {
            let Some(action) = layer.slots.get(name) else {
                continue;
            };
            let result = if action.kind.is_software() {
                software += 1;
                if software > agent::max_software_slots() {
                    Err(anyhow!(
                        "too many agent actions (max {})",
                        agent::max_software_slots()
                    ))
                } else {
                    agent::check_software(&action.kind)
                }
            } else if !caps.actions.contains(&action.kind.type_name()) {
                Err(anyhow!(
                    "{} keypads can't store {} actions",
                    m.model,
                    action.kind.type_name()
                ))
            } else {
                Slot::parse(name).and_then(|slot| driver.check_action(m, li, slot, &action.kind))
            };
            if let Err(e) = result {
                errors.insert(format!("{li}:{name}"), format!("{e:#}"));
            }
        }
    }
    Ok(errors)
}

/// A keymap ready to write, and the triggers the agent should listen for.
#[derive(Debug, Clone)]
pub struct Prepared {
    pub encoded: Encoded,
    pub triggers: Vec<Trigger>,
    /// Slots whose action is stored on the device itself.
    pub hardware: usize,
}

/// Checks a keymap, lowers its software actions, and encodes it.
pub fn prepare(driver: &dyn DeviceDriver, m: &Mapping) -> Result<Prepared> {
    if let Some((slot, e)) = slot_errors(driver, m)?.into_iter().next() {
        let (layer, name) = slot.split_once(':').unwrap_or(("0", &slot));
        let layer: usize = layer.parse().unwrap_or(0);
        bail!("layer {} {name}: {e}", layer + 1);
    }
    let (lowered, triggers) = agent::lower(m)?;
    let used: usize = m.layers.iter().map(|l| l.slots.len()).sum();
    Ok(Prepared {
        encoded: driver.encode(&lowered)?,
        hardware: used - triggers.len(),
        triggers,
    })
}
