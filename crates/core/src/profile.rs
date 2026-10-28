//! Profiles: named sets of device configs ("Halo", "Adobe").
//!
//! A profile holds one keymap per physical device. Applying it writes each
//! keymap to its device, when that device is attached; devices the profile
//! doesn't mention keep what they have.
//!
//! A config is bound to a device by driver, model and USB id, then by serial
//! number when the device reports one. CH57x pads report none, so identical
//! pads are told apart by their position among identical attached pads
//! (sorted by system path). hidapi paths themselves aren't stable across
//! replugs, so they're never stored.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::driver::DetectedDevice;
use crate::model::Mapping;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Profiles {
    /// The profile last applied or selected.
    #[serde(default)]
    pub active: Option<String>,
    #[serde(default)]
    pub profiles: Vec<Profile>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub configs: Vec<DeviceConfig>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeviceConfig {
    pub id: String,
    /// What the user calls this device ("Left pad").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The device this keymap is for; `None` means the first attached device
    /// of the keymap's driver and model that no other config claims.
    #[serde(default)]
    pub bind: Option<DeviceRef>,
    pub keymap: Mapping,
}

/// Identifies one physical device among those attached.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DeviceRef {
    pub vendor_id: u16,
    pub product_id: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub serial: Option<String>,
    /// Position among attached devices with the same driver and USB id and
    /// no serial, ordered by system path. 0 for the only one.
    #[serde(default)]
    pub index: usize,
}

impl DeviceRef {
    /// A stable key for this device under `driver`, used to remember what's on it.
    pub fn key(&self, driver: &str) -> String {
        match &self.serial {
            Some(s) => format!(
                "{driver}:{:04x}:{:04x}:s:{s}",
                self.vendor_id, self.product_id
            ),
            None => format!(
                "{driver}:{:04x}:{:04x}:#{}",
                self.vendor_id, self.product_id, self.index
            ),
        }
    }
}

/// Every attached device with the [`DeviceRef`] that identifies it.
pub fn identify(devices: &[DetectedDevice]) -> Vec<(DeviceRef, DetectedDevice)> {
    let mut sorted: Vec<&DetectedDevice> = devices.iter().collect();
    sorted.sort_by(|a, b| a.path.cmp(&b.path));
    let mut out: Vec<(DeviceRef, DetectedDevice)> = Vec::new();
    for d in sorted {
        let index = if d.serial.is_some() {
            0
        } else {
            out.iter()
                .filter(|(r, o)| {
                    o.driver == d.driver
                        && r.vendor_id == d.vendor_id
                        && r.product_id == d.product_id
                        && r.serial.is_none()
                })
                .count()
        };
        let r = DeviceRef {
            vendor_id: d.vendor_id,
            product_id: d.product_id,
            serial: d.serial.clone(),
            index,
        };
        out.push((r, d.clone()));
    }
    out
}

/// Where one config of a profile goes.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Resolved {
    pub config: String,
    /// The device and the key that identifies it, or why there's none.
    pub target: Result<(String, DetectedDevice), String>,
}

/// Matches each config of `profile` to an attached device. Bound configs
/// are placed first, so an unbound config never takes a bound one's device.
pub fn resolve(profile: &Profile, devices: &[DetectedDevice]) -> Vec<Resolved> {
    let attached = identify(devices);
    let mut claimed: HashSet<usize> = HashSet::new();
    let fits = |c: &DeviceConfig, d: &DetectedDevice| {
        d.driver == c.keymap.driver && d.models.contains(&c.keymap.model.as_str())
    };

    let mut out: Vec<Option<Resolved>> = vec![None; profile.configs.len()];
    for (ci, c) in profile
        .configs
        .iter()
        .enumerate()
        .filter(|(_, c)| c.bind.is_some())
    {
        let want = c.bind.as_ref().expect("filtered on bind");
        let found = attached
            .iter()
            .position(|(r, d)| r == want && d.driver == c.keymap.driver);
        let target = match found {
            Some(i) if claimed.contains(&i) => {
                Err("another config in this profile is bound to the same device".into())
            }
            Some(i) if !fits(c, &attached[i].1) => Err(format!(
                "the bound device speaks {}, but this keymap is for {}",
                attached[i].1.models.join(" or "),
                c.keymap.model
            )),
            Some(i) => {
                claimed.insert(i);
                Ok((attached[i].0.key(&c.keymap.driver), attached[i].1.clone()))
            }
            None => Err("not connected".into()),
        };
        out[ci] = Some(Resolved {
            config: c.id.clone(),
            target,
        });
    }
    for (ci, c) in profile
        .configs
        .iter()
        .enumerate()
        .filter(|(_, c)| c.bind.is_none())
    {
        let found = (0..attached.len()).find(|i| !claimed.contains(i) && fits(c, &attached[*i].1));
        let target = match found {
            Some(i) => {
                claimed.insert(i);
                Ok((attached[i].0.key(&c.keymap.driver), attached[i].1.clone()))
            }
            None => Err(format!("no {} device attached", c.keymap.model)),
        };
        out[ci] = Some(Resolved {
            config: c.id.clone(),
            target,
        });
    }
    out.into_iter()
        .map(|r| r.expect("every config resolved"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pad(path: &str, pid: u16) -> DetectedDevice {
        DetectedDevice {
            driver: "ch57x",
            path: path.into(),
            vendor_id: 0x1189,
            product_id: pid,
            name: "pad".into(),
            serial: None,
            models: vec!["ch57x-1"],
        }
    }

    fn config(id: &str, bind: Option<DeviceRef>) -> DeviceConfig {
        let keymap: Mapping = serde_json::from_str(
            r#"{"rows": 3, "columns": 4, "knobs": 2, "layers": [{"slots": {}}]}"#,
        )
        .unwrap();
        DeviceConfig {
            id: id.into(),
            name: None,
            bind,
            keymap,
        }
    }

    fn r(index: usize) -> DeviceRef {
        DeviceRef {
            vendor_id: 0x1189,
            product_id: 0x8840,
            serial: None,
            index,
        }
    }

    #[test]
    fn identical_pads_are_numbered_by_path() {
        let ids = identify(&[
            pad("/dev/hidraw7", 0x8840),
            pad("/dev/hidraw3", 0x8840),
            pad("/dev/hidraw5", 0x8842),
        ]);
        let by_path: Vec<(&str, usize)> = ids
            .iter()
            .map(|(r, d)| (d.path.as_str(), r.index))
            .collect();
        assert_eq!(
            by_path,
            [
                ("/dev/hidraw3", 0),
                ("/dev/hidraw5", 0),
                ("/dev/hidraw7", 1)
            ]
        );
    }

    #[test]
    fn a_two_pad_profile_reaches_both_pads() {
        let halo = Profile {
            id: "halo".into(),
            name: "Halo".into(),
            configs: vec![config("left", Some(r(0))), config("right", Some(r(1)))],
        };
        let devices = [pad("/dev/hidraw2", 0x8840), pad("/dev/hidraw4", 0x8840)];
        let res = resolve(&halo, &devices);
        assert_eq!(res[0].target.as_ref().unwrap().1.path, "/dev/hidraw2");
        assert_eq!(res[1].target.as_ref().unwrap().1.path, "/dev/hidraw4");
        assert_ne!(
            res[0].target.as_ref().unwrap().0,
            res[1].target.as_ref().unwrap().0
        );
    }

    #[test]
    fn missing_devices_are_reported_not_fatal() {
        let p = Profile {
            id: "p".into(),
            name: "P".into(),
            configs: vec![config("a", Some(r(0))), config("b", Some(r(1)))],
        };
        let res = resolve(&p, &[pad("/dev/hidraw2", 0x8840)]);
        assert!(res[0].target.is_ok());
        assert_eq!(res[1].target.as_ref().unwrap_err(), "not connected");
    }

    #[test]
    fn unbound_configs_take_what_bound_ones_leave() {
        let p = Profile {
            id: "p".into(),
            name: "P".into(),
            configs: vec![config("any", None), config("second", Some(r(0)))],
        };
        let res = resolve(
            &p,
            &[pad("/dev/hidraw2", 0x8840), pad("/dev/hidraw4", 0x8840)],
        );
        assert_eq!(res[1].target.as_ref().unwrap().1.path, "/dev/hidraw2");
        assert_eq!(res[0].target.as_ref().unwrap().1.path, "/dev/hidraw4");
    }

    #[test]
    fn serials_beat_positions() {
        let mut a = pad("/dev/hidraw9", 0x8840);
        a.serial = Some("ABC".into());
        let want = DeviceRef {
            serial: Some("ABC".into()),
            ..r(0)
        };
        let p = Profile {
            id: "p".into(),
            name: "P".into(),
            configs: vec![config("s", Some(want))],
        };
        let res = resolve(&p, &[pad("/dev/hidraw1", 0x8840), a]);
        assert_eq!(res[0].target.as_ref().unwrap().1.path, "/dev/hidraw9");
    }
}
