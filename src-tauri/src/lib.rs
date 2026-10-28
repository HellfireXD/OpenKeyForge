//! The app layer: Tauri commands for the editor. Everything device-specific
//! goes through the driver [`Registry`]; this file never names a device family
//! except to register its driver in [`registry`].

mod agent;
mod hid;
mod store;

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use okf_core::agent::{check_software, max_software_slots};
use okf_core::driver::{prepare, slot_errors, DetectedDevice, DeviceDriver, ModelInfo};
#[cfg(target_os = "linux")]
use okf_core::input::KeypadEvent;
use okf_core::model::{Action, ActionKind, MacroStep};
use okf_core::{keys, Mapping, Registry};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;

use agent::{Agent, AgentStatus};
use hid::Hid;
use store::Store;

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
}

impl AppState {
    fn driver(&self, m: &Mapping) -> Result<Arc<dyn DeviceDriver>, String> {
        self.registry.get(&m.driver).map_err(err)
    }
}

fn err(e: anyhow::Error) -> String {
    format!("{e:#}")
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
    mapping: Mapping,
    uploaded: Option<Mapping>,
    warning: Option<String>,
    errors: BTreeMap<String, String>,
    catalog: Catalog,
}

#[tauri::command]
fn get_state(state: State<'_, AppState>) -> Result<StateDto, String> {
    let registry = &state.registry;
    let starter = || {
        registry
            .get(DEFAULT_DRIVER)
            .and_then(|d| d.starter(DEFAULT_MODEL))
            .expect("the default driver has a starter keymap")
    };
    let check = |m: &Mapping| registry.get(&m.driver)?.check_layout(m);
    let (mapping, warning) = state.store.draft(starter, check);
    let errors = errors_for(state.driver(&mapping)?.as_ref(), &mapping)?;
    Ok(StateDto {
        mapping,
        uploaded: state.store.uploaded(),
        warning,
        errors,
        catalog: catalog(registry),
    })
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

#[derive(Serialize)]
struct ErrorsDto {
    errors: BTreeMap<String, String>,
}

#[tauri::command]
fn save_mapping(state: State<'_, AppState>, mapping: Mapping) -> Result<ErrorsDto, String> {
    let errors = errors_for(state.driver(&mapping)?.as_ref(), &mapping)?;
    state.store.save_draft(&mapping).map_err(err)?;
    Ok(ErrorsDto { errors })
}

#[derive(Serialize)]
struct PreviewDto {
    packets: Vec<String>,
    hardware: usize,
    software: usize,
}

#[tauri::command]
fn preview(state: State<'_, AppState>, mapping: Mapping) -> Result<PreviewDto, String> {
    let p = prepare(state.driver(&mapping)?.as_ref(), &mapping).map_err(err)?;
    Ok(PreviewDto {
        packets: p.encoded.preview,
        hardware: p.hardware,
        software: p.triggers.len(),
    })
}

#[derive(Serialize)]
struct UploadDto {
    hardware: usize,
    software: usize,
}

/// The single device of this keymap's driver to write to, or why not.
fn pick(
    driver: &dyn DeviceDriver,
    api: &okf_core::hidapi::HidApi,
    m: &Mapping,
    path: Option<&str>,
) -> anyhow::Result<DetectedDevice> {
    let found = driver.detect(api);
    let device = match path {
        Some(path) => found.into_iter().find(|d| d.path == path),
        None if found.len() > 1 => {
            anyhow::bail!("several {} are connected; choose one", driver.name())
        }
        None => found.into_iter().next(),
    };
    let Some(device) = device else {
        anyhow::bail!(
            "no {} found{}",
            driver.name(),
            if cfg!(target_os = "linux") {
                "; on Linux, install the udev rule and replug the device"
            } else {
                ""
            }
        );
    };
    anyhow::ensure!(
        device.models.contains(&m.model.as_str()),
        "this device ({}) speaks {}, but the keymap is for {}; change the model under Device",
        device.usb_id(),
        device.models.join(" or "),
        m.model
    );
    Ok(device)
}

#[tauri::command]
async fn upload(
    app: AppHandle,
    state: State<'_, AppState>,
    mapping: Mapping,
    path: Option<String>,
) -> Result<UploadDto, String> {
    let driver = state.driver(&mapping)?;
    if !errors_for(driver.as_ref(), &mapping)?.is_empty() {
        return Err("Fix the slots marked in red first".into());
    }
    let prepared = prepare(driver.as_ref(), &mapping).map_err(err)?;
    let (m, encoded) = (mapping.clone(), prepared.encoded.clone());
    state.hid.call(move |api| {
        let device = pick(driver.as_ref(), api, &m, path.as_deref())?;
        driver.write(api, &device, &encoded)
    })?;
    state.store.save_uploaded(&mapping).map_err(err)?;
    let software = prepared.triggers.len();
    state.agent.set_triggers(&app, prepared.triggers);
    Ok(UploadDto {
        hardware: prepared.hardware,
        software,
    })
}

/// Reads the keymap stored on the device, for drivers whose devices can report it.
#[tauri::command]
async fn read_from_device(
    state: State<'_, AppState>,
    driver: String,
    model: String,
    path: Option<String>,
) -> Result<Mapping, String> {
    let driver = state.registry.get(&driver).map_err(err)?;
    state.hid.call(move |api| {
        let probe = driver.starter(&model)?;
        let device = pick(driver.as_ref(), api, &probe, path.as_deref())?;
        driver.read_keymap(api, &device)
    })
}

#[derive(Serialize)]
struct StatusDto {
    devices: Vec<DetectedDevice>,
    agent: AgentStatus,
    watching: bool,
}

#[tauri::command]
fn status(state: State<'_, AppState>) -> StatusDto {
    StatusDto {
        devices: state.hid.devices.lock().unwrap().clone(),
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
    state
        .store
        .save_settings(&store::Settings { agent: enabled })
        .map_err(err)?;
    Ok(state.agent.status())
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
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
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
        .setup(|app| {
            let registry = registry();
            let store = Store::new(app.path().app_data_dir()?);
            let agent = Arc::new(Agent::default());

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

            // Agent slots from the last upload keep working after a restart.
            let last = store.uploaded().and_then(|m| {
                let driver = registry.get(&m.driver).ok()?;
                prepare(driver.as_ref(), &m).ok()
            });
            if let Some(prepared) = last {
                agent.set_triggers(app.handle(), prepared.triggers);
            }
            agent.set_enabled(app.handle(), store.settings().agent);

            app.manage(AppState {
                registry,
                hid,
                agent,
                store,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_state,
            starter_mapping,
            save_mapping,
            preview,
            upload,
            read_from_device,
            status,
            set_agent,
            test_action,
            list_apps,
            import_mapping,
            export_mapping,
        ])
        .run(tauri::generate_context!())
        .expect("error while running OpenKeyForge");
}
