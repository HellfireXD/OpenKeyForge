//! Finding keypads and writing packets to them through hidapi.
//!
//! Configuration goes to the keypad's vendor interface (usage page 0xFF00,
//! report id 3). Unlike libusb this needs no kernel-driver detach on Linux,
//! no WinUSB driver swap on Windows, and works with macOS's HID driver.
//!
//! On macOS hidapi binds to the run loop of the thread that first uses it, so
//! callers that are multi-threaded (the app) keep one `HidApi` on one
//! long-lived thread.

use std::ffi::CString;

use anyhow::{anyhow, bail, ensure, Context as _, Result};
use hidapi::{DeviceInfo, HidApi, HidDevice};
use serde::Serialize;

use crate::config::{KeyboardModel, SUPPORTED_DEVICES};

pub const VENDOR_USAGE_PAGE: u16 = 0xFF00;
pub const PACKET: usize = 64;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Keypad {
    /// hidapi path of the vendor interface; pass back to [`send`].
    pub path: String,
    pub vendor_id: u16,
    pub product_id: u16,
    pub product: String,
    /// USB serial number, if the device reports one (CH57x pads usually don't).
    pub serial: Option<String>,
    /// Models that use this USB id. More than one means the user must pick.
    pub models: Vec<KeyboardModel>,
}

impl Keypad {
    pub fn id(&self) -> String {
        format!("{:04x}:{:04x}", self.vendor_id, self.product_id)
    }
}

pub fn is_supported(vendor_id: u16, product_id: u16) -> bool {
    SUPPORTED_DEVICES.iter().any(|(_, v, p, _)| *v == vendor_id && *p == product_id)
}

/// Every supported keypad's vendor interface.
pub fn list(api: &HidApi) -> Vec<Keypad> {
    let mut out: Vec<Keypad> = api
        .device_list()
        .filter(|d| is_supported(d.vendor_id(), d.product_id()) && d.usage_page() == VENDOR_USAGE_PAGE)
        .map(|d| Keypad {
            path: d.path().to_string_lossy().into_owned(),
            vendor_id: d.vendor_id(),
            product_id: d.product_id(),
            product: d.product_string().unwrap_or("").trim().to_string(),
            serial: d.serial_number().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string),
            models: KeyboardModel::from_vid_pid(d.vendor_id(), d.product_id())
                .into_iter()
                .map(|i| i.model)
                .collect(),
        })
        .collect();
    // Windows lists one entry per top-level collection; keep one per path.
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out.dedup_by(|a, b| a.path == b.path);
    out
}

/// The keypad's other interfaces (keyboard, consumer, mouse): what it sends
/// when a key is pressed. Read by the live key test and the Linux agent.
pub fn input_interfaces(api: &HidApi) -> Vec<&DeviceInfo> {
    api.device_list()
        .filter(|d| is_supported(d.vendor_id(), d.product_id()) && d.usage_page() != VENDOR_USAGE_PAGE)
        .collect()
}

/// The single keypad to talk to, or an error that says what to do.
pub fn pick(api: &HidApi, path: Option<&str>) -> Result<Keypad> {
    let found = list(api);
    if let Some(path) = path {
        return found
            .into_iter()
            .find(|k| k.path == path)
            .ok_or_else(|| anyhow!("no keypad at {path}; run `list` to see what is connected"));
    }
    match found.len() {
        0 => bail!(
            "CH57x keypad not found. Supported USB ids: {}.{}",
            SUPPORTED_DEVICES.iter().map(|(_, v, p, _)| format!("{v:04x}:{p:04x}")).collect::<Vec<_>>().join(", "),
            if cfg!(target_os = "linux") {
                " On Linux, install the udev rule (packaging/linux/70-openkeyforge.rules) and replug the keypad."
            } else {
                ""
            }
        ),
        1 => Ok(found.into_iter().next().unwrap()),
        _ => bail!("several keypads are connected; choose one by path:\n{}", found.iter().map(|k| format!("  {}  {}", k.id(), k.path)).collect::<Vec<_>>().join("\n")),
    }
}

fn write(dev: &HidDevice, data: &[u8]) -> Result<()> {
    let n = dev.write(data).context("HID write")?;
    ensure!(n >= data.len().min(PACKET), "HID write was short ({n} of {} bytes)", data.len());
    Ok(())
}

/// Writes `packets` (64-byte chunks whose first byte, 0x03, is the report id).
pub fn send(api: &HidApi, path: &str, packets: &[u8]) -> Result<()> {
    ensure!(packets.len().is_multiple_of(PACKET), "packets must be 64-byte chunks");
    let cpath = CString::new(path).context("device path")?;
    let dev = api.open_path(&cpath).context("open keypad (is another program using it?)")?;

    // The original tool sends one all-zero packet first. hidapi reads the
    // leading 0 as "no report id". The keypad doesn't need it (WebHID uploads
    // skip it) and Windows rejects a report the device doesn't declare, so a
    // failure here is not an error.
    let _ = dev.write(&[0u8; PACKET + 1]);

    for chunk in packets.chunks(PACKET) {
        log::debug!("send: {chunk:02x?}");
        write(&dev, chunk)?;
    }
    Ok(())
}
