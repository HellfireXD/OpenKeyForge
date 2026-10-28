//! The app layer: Tauri commands for the editor. Everything device-specific
//! goes through the driver [`Registry`]; this file never names a device family
//! except to register its driver in [`registry`].
//!
//! The user edits profiles. Each profile holds one keymap per device
//! ("config"); writing a config records what that device now holds in the
//! live map, keyed by device. The agent listens for the chords of every live
//! device, and new chords are always picked outside those already live, so
//! two devices never send the same chord for different actions.

mod agent;
mod hid;
mod store;
mod tray;

use std::collections::{BTreeMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use okf_core::agent::{check_software, max_software_slots, Trigger};
use okf_core::driver::{prepare_avoiding, slot_errors, DetectedDevice, DeviceDriver, ModelInfo};
#[cfg(target_os = "linux")]
use okf_core::input::KeypadEvent;
use okf_core::model::{Action, ActionKind, MacroStep};
use okf_core::profile::{identify, resolve, DeviceConfig, DeviceRef, Profile, Profiles};
use okf_core::{keys, Mapping, Registry};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;

use agent::{Agent, AgentStatus};
use hid::Hid;
use store::{Live, LiveMap, Settings, Store};

/// Every device family OpenKeyForge can program. A new driver is one line here.
fn registry() -> Registry {
    Registry::new(vec![Arc::new(okf_driver_ch57x::Ch57xDriver)])
}

/// What a fresh install edits before any device is chosen.
const DEFAULT_DRIVER: &str = okf_driver_ch57x::DRIVER_ID;
const DEFAULT_MODEL: &str = "ch57x-1";

struct AppState {
    registry: Registry,
    hid: Hid,
    agent: Arc<Agent>,
    store: Store,
    profiles: Mutex<Profiles>,
    live: Mutex<LiveMap>,
    settings: Mutex<Settings>,
    /// One device write at a time (tray and window can both start one).
    writing: Mutex<()>,
    warning: Option<String>,
}

impl AppState {
    fn driver(&self, m: &Mapping) -> Result<Arc<dyn DeviceDriver>, String> {
        self.registry.get(&m.driver).map_err(err)
    }

    fn save_profiles(&self, p: &Profiles) -> Result<(), String> {
        self.store.save_profiles(p).map_err(err)
    }

    fn profile(&self, id: &str) -> Result<Profile, String> {
        let profiles = self.profiles.lock().unwrap();
        profiles
            .profiles
            .iter()
            .find(|p| p.id == id)
            .cloned()
            .ok_or_else(|| format!("no profile {id:?}"))
    }

    /// Every agent chord a live device sends.
    fn live_triggers(&self) -> Vec<Trigger> {
        self.live
            .lock()
            .unwrap()
            .values()
            .flat_map(|l| l.triggers.clone())
            .collect()
    }
}

fn err(e: anyhow::Error) -> String {
    format!("{e:#}")
}

fn new_id(prefix: &str) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{prefix}{nanos:x}")
}

#[derive(Serialize)]
struct DriverDto {
    id: &'static str,
    name: &'static str,
    models: Vec<ModelInfo>,
}

#[derive(Serialize)]
struct Catalog {
    drivers: Vec<DriverDto>,
    modifiers: &'static [&'static str],
    keys: &'static [&'static str],
    media: Vec<&'static str>,
    software_slots: usize,
    platform: &'static str,
    shell: &'static str,
}

fn catalog(registry: &Registry) -> Catalog {
    let platform = if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(windows) {
        "windows"
    } else {
        "linux"
    };
    Catalog {
        drivers: registry
            .all()
            .iter()
            .map(|d| DriverDto {
                id: d.id(),
                name: d.name(),
                models: d.models(),
            })
            .collect(),
        modifiers: keys::MODIFIER_NAMES,
        keys: keys::KEY_NAMES,
        media: keys::MEDIA.iter().map(|(name, _)| *name).collect(),
        software_slots: max_software_slots(),
        platform,
        shell: agent::shell_name(),
    }
}

/// Slot errors, plus keystroke steps the agent couldn't send on this platform.
fn errors_for(driver: &dyn DeviceDriver, m: &Mapping) -> Result<BTreeMap<String, String>, String> {
    let mut errors = slot_errors(driver, m).map_err(err)?;
    for (li, layer) in m.layers.iter().enumerate() {
        for (slot, action) in &layer.slots {
            let ActionKind::Macro { steps } = &action.kind else {
                continue;
            };
            for step in steps {
                if let MacroStep::Keys { value, .. } = step {
                    if let Err(e) = agent::check_sendable(value) {
                        errors
                            .entry(format!("{li}:{slot}"))
                            .or_insert_with(|| e.to_string());
                    }
                }
            }
        }
    }
    Ok(errors)
}

#[derive(Serialize)]
struct StateDto {
    profiles: Profiles,
    /// What each config last wrote to its device, by config id, if that
    /// device still holds it.
    applied: BTreeMap<String, Mapping>,
    warning: Option<String>,
    settings: Settings,
    catalog: Catalog,
}

fn applied(live: &LiveMap) -> BTreeMap<String, Mapping> {
    live.values()
        .map(|l| (l.config.clone(), l.keymap.clone()))
        .collect()
}

#[tauri::command]
fn get_state(state: State<'_, AppState>) -> StateDto {
    StateDto {
        profiles: state.profiles.lock().unwrap().clone(),
        applied: applied(&state.live.lock().unwrap()),
        warning: state.warning.clone(),
        settings: state.settings.lock().unwrap().clone(),
        catalog: catalog(&state.registry),
    }
}

/// Slot errors for a keymap, for the editor's red marks.
#[tauri::command]
fn check_keymap(
    state: State<'_, AppState>,
    keymap: Mapping,
) -> Result<BTreeMap<String, String>, String> {
    errors_for(state.driver(&keymap)?.as_ref(), &keymap)
}

/// A new keymap for a driver and model, for when the user switches device type.
#[tauri::command]
fn starter_mapping(
    state: State<'_, AppState>,
    driver: String,
    model: String,
) -> Result<Mapping, String> {
    state
        .registry
        .get(&driver)
        .and_then(|d| d.starter(&model))
        .map_err(err)
}

fn edit_profiles<T>(
    state: &AppState,
    app: &AppHandle,
    f: impl FnOnce(&mut Profiles) -> Result<T, String>,
) -> Result<T, String> {
    let mut profiles = state.profiles.lock().unwrap();
    let out = f(&mut profiles)?;
    state.save_profiles(&profiles)?;
    let snapshot = profiles.clone();
    drop(profiles);
    let _ = tray::sync(app, state.settings.lock().unwrap().tray, &snapshot);
    let _ = app.emit("profiles-changed", &snapshot);
    Ok(out)
}

fn find_config<'a>(
    profiles: &'a mut Profiles,
    profile: &str,
    config: &str,
) -> Result<&'a mut DeviceConfig, String> {
    profiles
        .profiles
        .iter_mut()
        .find(|p| p.id == profile)
        .and_then(|p| p.configs.iter_mut().find(|c| c.id == config))
        .ok_or_else(|| "that device config no longer exists".to_string())
}

/// Saves the keymap being edited and returns its slot errors.
#[tauri::command]
fn save_config(
    app: AppHandle,
    state: State<'_, AppState>,
    profile: String,
    config: String,
    keymap: Mapping,
) -> Result<BTreeMap<String, String>, String> {
    let errors = errors_for(state.driver(&keymap)?.as_ref(), &keymap)?;
    // Saving happens on every edit, so skip the tray/menu refresh.
    let mut profiles = state.profiles.lock().unwrap();
    find_config(&mut profiles, &profile, &config)?.keymap = keymap;
    state.save_profiles(&profiles)?;
    let _ = app;
    Ok(errors)
}

#[tauri::command]
fn create_profile(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
    copy_of: Option<String>,
) -> Result<Profile, String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("give the profile a name".into());
    }
    let starter = || {
        state
            .registry
            .get(DEFAULT_DRIVER)
            .and_then(|d| d.starter(DEFAULT_MODEL))
            .map_err(err)
    };
    edit_profiles(&state, &app, |profiles| {
        let configs =
            match copy_of.and_then(|id| profiles.profiles.iter().find(|p| p.id == id).cloned()) {
                Some(src) => src
                    .configs
                    .into_iter()
                    .map(|c| DeviceConfig {
                        id: new_id("c"),
                        ..c
                    })
                    .collect(),
                None => vec![DeviceConfig {
                    id: new_id("c"),
                    name: None,
                    bind: None,
                    keymap: starter()?,
                }],
            };
        let profile = Profile {
            id: new_id("p"),
            name,
            configs,
        };
        profiles.profiles.push(profile.clone());
        Ok(profile)
    })
}

#[tauri::command]
fn rename_profile(
    app: AppHandle,
    state: State<'_, AppState>,
    profile: String,
    name: String,
) -> Result<(), String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("give the profile a name".into());
    }
    edit_profiles(&state, &app, |profiles| {
        let p = profiles
            .profiles
            .iter_mut()
            .find(|p| p.id == profile)
            .ok_or("no such profile")?;
        p.name = name;
        Ok(())
    })
}

#[tauri::command]
fn delete_profile(
    app: AppHandle,
    state: State<'_, AppState>,
    profile: String,
) -> Result<(), String> {
    edit_profiles(&state, &app, |profiles| {
        if profiles.profiles.len() <= 1 {
            return Err("keep at least one profile".into());
        }
        profiles.profiles.retain(|p| p.id != profile);
        if profiles.active.as_deref() == Some(profile.as_str()) {
            profiles.active = None;
        }
        Ok(())
    })
}

/// Adds a device config to a profile: bound to an attached device (`bind`),
/// or to whichever matching device is attached (`None`).
#[tauri::command]
fn add_config(
    app: AppHandle,
    state: State<'_, AppState>,
    profile: String,
    bind: Option<DeviceRef>,
    driver: String,
    model: String,
) -> Result<DeviceConfig, String> {
    let keymap = state
        .registry
        .get(&driver)
        .and_then(|d| d.starter(&model))
        .map_err(err)?;
    edit_profiles(&state, &app, |profiles| {
        let p = profiles
            .profiles
            .iter_mut()
            .find(|p| p.id == profile)
            .ok_or("no such profile")?;
        let config = DeviceConfig {
            id: new_id("c"),
            name: None,
            bind,
            keymap,
        };
        p.configs.push(config.clone());
        Ok(config)
    })
}

#[tauri::command]
fn update_config(
    app: AppHandle,
    state: State<'_, AppState>,
    profile: String,
    config: String,
    name: Option<String>,
    bind: Option<DeviceRef>,
) -> Result<(), String> {
    edit_profiles(&state, &app, |profiles| {
        let c = find_config(profiles, &profile, &config)?;
        c.name = name.map(|n| n.trim().to_string()).filter(|n| !n.is_empty());
        c.bind = bind;
        Ok(())
    })
}

#[tauri::command]
fn remove_config(
    app: AppHandle,
    state: State<'_, AppState>,
    profile: String,
    config: String,
) -> Result<(), String> {
    edit_profiles(&state, &app, |profiles| {
        let p = profiles
            .profiles
            .iter_mut()
            .find(|p| p.id == profile)
            .ok_or("no such profile")?;
        if p.configs.len() <= 1 {
            return Err("a profile needs at least one device".into());
        }
        p.configs.retain(|c| c.id != config);
        Ok(())
    })
}

#[derive(Serialize)]
struct PreviewDto {
    packets: Vec<String>,
    hardware: usize,
    software: usize,
}

#[tauri::command]
fn preview(state: State<'_, AppState>, keymap: Mapping) -> Result<PreviewDto, String> {
    let driver = state.driver(&keymap)?;
    let p = prepare_avoiding(driver.as_ref(), &keymap, &HashSet::new()).map_err(err)?;
    Ok(PreviewDto {
        packets: p.encoded.preview,
        hardware: p.hardware,
        software: p.triggers.len(),
    })
}

#[derive(Debug, Clone, Serialize)]
struct WriteResult {
    config: String,
    /// Slots stored on the device, and slots run by the agent.
    hardware: usize,
    software: usize,
    /// Why the config wasn't written (device missing, invalid keymap, …).
    error: Option<String>,
}

/// Writes one config to its resolved device and records it as live there.
fn write_config(
    state: &AppState,
    app: &AppHandle,
    profile: &Profile,
    config: &DeviceConfig,
    key: String,
    device: DetectedDevice,
) -> Result<(usize, usize), String> {
    let driver = state.driver(&config.keymap)?;
    if !errors_for(driver.as_ref(), &config.keymap)?.is_empty() {
        return Err("fix the slots marked in red first".into());
    }
    let taken: HashSet<(u8, u8)> = state
        .live
        .lock()
        .unwrap()
        .iter()
        .filter(|(k, _)| **k != key)
        .flat_map(|(_, l)| l.triggers.iter().map(|t| (t.mods, t.code)))
        .collect();
    let prepared = prepare_avoiding(driver.as_ref(), &config.keymap, &taken).map_err(err)?;
    let encoded = prepared.encoded.clone();
    state
        .hid
        .call(move |api| driver.write(api, &device, &encoded))?;

    let software = prepared.triggers.len();
    let mut live = state.live.lock().unwrap();
    live.insert(
        key,
        Live {
            keymap: config.keymap.clone(),
            triggers: prepared.triggers,
            profile: profile.id.clone(),
            config: config.id.clone(),
        },
    );
    state.store.save_live(&live).map_err(err)?;
    drop(live);
    state.agent.set_triggers(app, state.live_triggers());
    Ok((prepared.hardware, software))
}

/// Writes `configs` of `profile` (all when `None`) to their attached devices.
fn apply(
    state: &AppState,
    app: &AppHandle,
    profile_id: &str,
    only: Option<&str>,
) -> Result<Vec<WriteResult>, String> {
    let _guard = state.writing.lock().unwrap_or_else(|e| e.into_inner());
    let profile = state.profile(profile_id)?;
    let devices = state.hid.devices.lock().unwrap().clone();
    let mut results = Vec::new();
    for (config, target) in profile.configs.iter().zip(resolve(&profile, &devices)) {
        if only.is_some_and(|id| id != config.id) {
            continue;
        }
        let outcome = target
            .target
            .and_then(|(key, device)| write_config(state, app, &profile, config, key, device));
        results.push(match outcome {
            Ok((hardware, software)) => WriteResult {
                config: config.id.clone(),
                hardware,
                software,
                error: None,
            },
            Err(e) => WriteResult {
                config: config.id.clone(),
                hardware: 0,
                software: 0,
                error: Some(e),
            },
        });
    }
    if only.is_none() {
        let mut profiles = state.profiles.lock().unwrap();
        profiles.active = Some(profile_id.to_string());
        state.save_profiles(&profiles)?;
        let snapshot = profiles.clone();
        drop(profiles);
        let _ = tray::sync(app, state.settings.lock().unwrap().tray, &snapshot);
        let _ = app.emit("profiles-changed", &snapshot);
    }
    let _ = app.emit(
        "applied",
        (&profile.id, &results, applied(&state.live.lock().unwrap())),
    );
    Ok(results)
}

/// Writes every config of a profile to the attached devices it names.
#[tauri::command]
async fn apply_profile(
    app: AppHandle,
    state: State<'_, AppState>,
    profile: String,
) -> Result<Vec<WriteResult>, String> {
    apply(&state, &app, &profile, None)
}

/// Writes one config (the one being edited) to its device.
#[tauri::command]
async fn upload_config(
    app: AppHandle,
    state: State<'_, AppState>,
    profile: String,
    config: String,
) -> Result<WriteResult, String> {
    let results = apply(&state, &app, &profile, Some(&config))?;
    let result = results
        .into_iter()
        .next()
        .ok_or("that device config no longer exists")?;
    match &result.error {
        Some(e) => Err(e.clone()),
        None => Ok(result),
    }
}

/// Called from the tray menu, off the UI thread.
pub(crate) fn apply_from_tray(app: &AppHandle, profile: &str) {
    let state = app.state::<AppState>();
    match apply(&state, app, profile, None) {
        Ok(results) => {
            for r in results.iter().filter(|r| r.error.is_some()) {
                log::warn!(
                    "profile {profile}: config {}: {}",
                    r.config,
                    r.error.as_deref().unwrap_or("")
                );
            }
        }
        Err(e) => log::warn!("profile {profile}: {e}"),
    }
}

/// Reads the keymap stored on the device, for drivers whose devices can report it.
#[tauri::command]
async fn read_from_device(
    state: State<'_, AppState>,
    driver: String,
    path: String,
) -> Result<Mapping, String> {
    let driver = state.registry.get(&driver).map_err(err)?;
    let device = state
        .hid
        .devices
        .lock()
        .unwrap()
        .iter()
        .find(|d| d.path == path)
        .cloned()
        .ok_or("that device is no longer connected")?;
    state.hid.call(move |api| driver.read_keymap(api, &device))
}

#[derive(Serialize)]
struct AttachedDto {
    #[serde(flatten)]
    device: DetectedDevice,
    /// How a config binds to this device.
    bind: DeviceRef,
    key: String,
}

#[derive(Serialize)]
struct StatusDto {
    devices: Vec<AttachedDto>,
    /// Where each config of each profile would go right now: profile id →
    /// config id → device key, or the reason it has none.
    targets: BTreeMap<String, BTreeMap<String, Result<String, String>>>,
    agent: AgentStatus,
    watching: bool,
}

#[tauri::command]
fn status(state: State<'_, AppState>) -> StatusDto {
    let devices = state.hid.devices.lock().unwrap().clone();
    let targets = state
        .profiles
        .lock()
        .unwrap()
        .profiles
        .iter()
        .map(|p| {
            let per_config = resolve(p, &devices)
                .into_iter()
                .map(|r| (r.config, r.target.map(|(key, _)| key)))
                .collect();
            (p.id.clone(), per_config)
        })
        .collect();
    StatusDto {
        devices: identify(&devices)
            .into_iter()
            .map(|(bind, device)| AttachedDto {
                key: bind.key(device.driver),
                bind,
                device,
            })
            .collect(),
        targets,
        agent: state.agent.status(),
        watching: state
            .hid
            .watching
            .load(std::sync::atomic::Ordering::Relaxed),
    }
}

#[tauri::command]
fn set_agent(
    app: AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<AgentStatus, String> {
    state.agent.set_enabled(&app, enabled);
    let mut settings = state.settings.lock().unwrap();
    settings.agent = enabled;
    state.store.save_settings(&settings).map_err(err)?;
    Ok(state.agent.status())
}

/// Turns the system tray icon on or off.
#[tauri::command]
fn set_tray(app: AppHandle, state: State<'_, AppState>, enabled: bool) -> Result<(), String> {
    let profiles = state.profiles.lock().unwrap().clone();
    tray::sync(&app, enabled, &profiles)?;
    let mut settings = state.settings.lock().unwrap();
    settings.tray = enabled;
    state.store.save_settings(&settings).map_err(err)
}

/// Runs an agent action after 1.5 s, so the user can click the window
/// where they want to see the result.
#[tauri::command]
fn test_action(app: AppHandle, state: State<'_, AppState>, action: Action) -> Result<(), String> {
    check_software(&action.kind).map_err(err)?;
    if let ActionKind::Macro { steps } = &action.kind {
        for step in steps {
            if let MacroStep::Keys { value, .. } = step {
                agent::check_sendable(value).map_err(err)?;
            }
        }
    }
    state
        .agent
        .run(&app, action.kind, Duration::from_millis(1500));
    Ok(())
}

/// App names for the Open action's suggestions (macOS opens them by name).
#[tauri::command]
fn list_apps() -> Vec<String> {
    let mut apps = Vec::new();
    if cfg!(target_os = "macos") {
        let home = std::env::var("HOME").unwrap_or_default();
        for dir in [
            "/Applications".to_string(),
            "/System/Applications".into(),
            format!("{home}/Applications"),
        ] {
            let Ok(entries) = std::fs::read_dir(dir) else {
                continue;
            };
            for e in entries.flatten() {
                if let Some(name) = e.file_name().to_str().and_then(|n| n.strip_suffix(".app")) {
                    apps.push(name.to_string());
                }
            }
        }
    }
    apps.sort();
    apps.dedup();
    apps
}

/// Opens a JSON keymap, a macropad-studio mapping, or a ch57x-keyboard-tool YAML config.
#[tauri::command]
async fn import_mapping(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<Mapping>, String> {
    let Some(file) = app
        .dialog()
        .file()
        .add_filter("Keymap", &["json", "yaml", "yml"])
        .blocking_pick_file()
    else {
        return Ok(None);
    };
    let path = file.into_path().map_err(|e| e.to_string())?;
    let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let is_yaml = matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("yaml" | "yml")
    );
    let mapping = if is_yaml {
        okf_driver_ch57x::import::from_yaml(&text).map_err(err)?
    } else {
        serde_json::from_str::<Mapping>(&text).map_err(|e| format!("not a keymap: {e}"))?
    };
    state
        .driver(&mapping)?
        .check_layout(&mapping)
        .map_err(err)?;
    Ok(Some(mapping))
}

#[tauri::command]
async fn export_mapping(app: AppHandle, mapping: Mapping) -> Result<bool, String> {
    let Some(file) = app
        .dialog()
        .file()
        .add_filter("Keymap", &["json"])
        .set_file_name("openkeyforge-keymap.json")
        .blocking_save_file()
    else {
        return Ok(false);
    };
    let path = file.into_path().map_err(|e| e.to_string())?;
    let text = serde_json::to_string_pretty(&mapping).map_err(|e| e.to_string())? + "\n";
    store::write_atomic(&path, &text).map_err(err)?;
    Ok(true)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            tray::show_window(app);
        }))
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init());

    #[cfg(any(windows, target_os = "macos"))]
    let builder = builder.plugin(
        tauri_plugin_global_shortcut::Builder::new()
            .with_handler(|app, shortcut, event| {
                use tauri_plugin_global_shortcut::ShortcutState;
                if event.state == ShortcutState::Pressed {
                    if let Some((mods, code)) = agent::hotkey::chord(shortcut) {
                        app.state::<AppState>().agent.on_chord(app, mods, code);
                    }
                }
            })
            .build(),
    );

    builder
        // With the tray on, closing the window keeps the app (and its agent
        // and profile switching) running in the background.
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let state = window.app_handle().state::<AppState>();
                let tray = state.settings.lock().unwrap().tray;
                if tray && window.app_handle().tray_by_id("main").is_some() {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .setup(|app| {
            let registry = registry();
            let store = Store::new(app.path().app_data_dir()?);
            let agent = Arc::new(Agent::default());

            let starter = || {
                registry
                    .get(DEFAULT_DRIVER)
                    .and_then(|d| d.starter(DEFAULT_MODEL))
                    .expect("the default driver has a starter keymap")
            };
            let (profiles, warning) = store.profiles(starter);
            let live = store.live();
            let settings = store.settings();

            let handle = app.handle().clone();
            #[cfg(target_os = "linux")]
            let chords = agent.clone();
            let hid = Hid::start(registry.clone(), move |event| {
                #[cfg(target_os = "linux")]
                if let KeypadEvent::Key { mods, code } = event {
                    if okf_core::agent::is_trigger_chord(mods, code) {
                        chords.on_chord(&handle, mods, code);
                    }
                }
                let _ = handle.emit("keypad-event", &event);
            });

            // Agent slots on every device keep working after a restart.
            let triggers: Vec<Trigger> = live.values().flat_map(|l| l.triggers.clone()).collect();
            agent.set_triggers(app.handle(), triggers);
            agent.set_enabled(app.handle(), settings.agent);

            if settings.tray {
                if let Err(e) = tray::sync(app.handle(), true, &profiles) {
                    log::warn!("{e}");
                }
            }

            app.manage(AppState {
                registry,
                hid,
                agent,
                store,
                profiles: Mutex::new(profiles),
                live: Mutex::new(live),
                settings: Mutex::new(settings),
                writing: Mutex::new(()),
                warning,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_state,
            check_keymap,
            starter_mapping,
            save_config,
            create_profile,
            rename_profile,
            delete_profile,
            add_config,
            update_config,
            remove_config,
            preview,
            apply_profile,
            upload_config,
            read_from_device,
            status,
            set_agent,
            set_tray,
            test_action,
            list_apps,
            import_mapping,
            export_mapping,
        ])
        .run(tauri::generate_context!())
        .expect("error while running OpenKeyForge");
}
