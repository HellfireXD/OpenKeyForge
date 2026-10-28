//! Profiles, what each device currently holds, and settings, as JSON files
//! in the app's data directory. Writes go through a temp file and a rename
//! so a crash never leaves half a file behind.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};
use okf_core::agent::Trigger;
use okf_core::profile::{DeviceConfig, Profile, Profiles};
use okf_core::Mapping;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub agent: bool,
    /// Keep running in the system tray when the window is closed.
    #[serde(default)]
    pub tray: bool,
}

/// What was last written to one physical device, keyed by
/// [`okf_core::profile::DeviceRef::key`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Live {
    pub keymap: Mapping,
    /// The agent chords this device sends.
    pub triggers: Vec<Trigger>,
    pub profile: String,
    pub config: String,
}

pub type LiveMap = BTreeMap<String, Live>;

pub struct Store {
    dir: PathBuf,
}

impl Store {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }

    fn read<T: DeserializeOwned>(&self, name: &str) -> Result<Option<T>> {
        let path = self.path(name);
        match fs::read_to_string(&path) {
            Ok(text) => Ok(Some(
                serde_json::from_str(&text).with_context(|| format!("read {}", path.display()))?,
            )),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e).with_context(|| format!("read {}", path.display())),
        }
    }

    fn write<T: Serialize>(&self, name: &str, value: &T) -> Result<()> {
        fs::create_dir_all(&self.dir).with_context(|| format!("create {}", self.dir.display()))?;
        let path = self.path(name);
        write_atomic(&path, &(serde_json::to_string_pretty(value)? + "\n"))
    }

    /// The saved profiles, or one "Default" profile holding the draft from
    /// before profiles existed (mapping.json) or `starter`. A file that can't
    /// be read is reported and replaced in memory, never overwritten until
    /// the next save.
    pub fn profiles(&self, starter: impl Fn() -> Mapping) -> (Profiles, Option<String>) {
        let default = |keymap: Mapping| Profiles {
            active: Some("default".into()),
            profiles: vec![Profile {
                id: "default".into(),
                name: "Default".into(),
                configs: vec![DeviceConfig {
                    id: "c1".into(),
                    name: None,
                    bind: None,
                    keymap,
                }],
            }],
        };
        match self.read::<Profiles>("profiles.json") {
            Ok(Some(p)) if !p.profiles.is_empty() => (p, None),
            Ok(_) => {
                let legacy = self.read::<Mapping>("mapping.json").ok().flatten();
                (default(legacy.unwrap_or_else(&starter)), None)
            }
            Err(e) => (
                default(starter()),
                Some(format!("{e:#}; starting with a default profile")),
            ),
        }
    }

    pub fn save_profiles(&self, p: &Profiles) -> Result<()> {
        self.write("profiles.json", p)
    }

    pub fn live(&self) -> LiveMap {
        self.read("live.json").ok().flatten().unwrap_or_default()
    }

    pub fn save_live(&self, live: &LiveMap) -> Result<()> {
        self.write("live.json", live)
    }

    pub fn settings(&self) -> Settings {
        self.read("settings.json")
            .ok()
            .flatten()
            .unwrap_or_default()
    }

    pub fn save_settings(&self, s: &Settings) -> Result<()> {
        self.write("settings.json", s)
    }
}

pub fn write_atomic(path: &Path, text: &str) -> Result<()> {
    let tmp = path.with_extension(format!("{}.tmp", std::process::id()));
    fs::write(&tmp, text).with_context(|| format!("write {}", tmp.display()))?;
    fs::rename(&tmp, path).with_context(|| format!("replace {}", path.display()))
}
