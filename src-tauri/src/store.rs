//! The draft, the last uploaded mapping and settings, as JSON files in the
//! app's data directory. Writes go through a temp file and a rename so a
//! crash never leaves half a file behind.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};
use okf_core::Mapping;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub agent: bool,
}

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

    /// The draft, or `starter` and a warning when it can't be read or `check`
    /// (the driver's layout check) rejects it.
    pub fn draft(
        &self,
        starter: impl Fn() -> Mapping,
        check: impl Fn(&Mapping) -> Result<()>,
    ) -> (Mapping, Option<String>) {
        match self.read::<Mapping>("mapping.json") {
            Ok(Some(m)) => match check(&m) {
                Ok(()) => (m, None),
                Err(e) => (
                    starter(),
                    Some(format!(
                        "Saved draft is invalid ({e:#}); showing the starter mapping"
                    )),
                ),
            },
            Ok(None) => (starter(), None),
            Err(e) => (
                starter(),
                Some(format!("{e:#}; showing the starter mapping")),
            ),
        }
    }

    pub fn save_draft(&self, m: &Mapping) -> Result<()> {
        self.write("mapping.json", m)
    }

    pub fn uploaded(&self) -> Option<Mapping> {
        self.read("uploaded.json").ok().flatten()
    }

    pub fn save_uploaded(&self, m: &Mapping) -> Result<()> {
        self.write("uploaded.json", m)
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
