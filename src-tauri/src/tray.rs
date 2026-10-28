//! The optional system tray icon: switch profiles without opening the window.
//!
//! The menu lists every profile (the active one checked), then Open and
//! Quit. Choosing a profile applies it to whichever of its devices are
//! attached. While the tray is on, closing the window only hides it.
//!
//! Linux needs a StatusNotifier host (KDE, most desktops; GNOME with the
//! AppIndicator extension). Where there's none, building the tray fails and
//! the error is shown instead.

use tauri::menu::{CheckMenuItem, IsMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};

use okf_core::profile::Profiles;

const TRAY_ID: &str = "main";

fn build_menu(app: &AppHandle, profiles: &Profiles) -> tauri::Result<Menu<Wry>> {
    let mut items: Vec<Box<dyn IsMenuItem<Wry>>> = Vec::new();
    for p in &profiles.profiles {
        let active = profiles.active.as_deref() == Some(p.id.as_str());
        items.push(Box::new(CheckMenuItem::with_id(
            app,
            format!("profile:{}", p.id),
            &p.name,
            true,
            active,
            None::<&str>,
        )?));
    }
    items.push(Box::new(PredefinedMenuItem::separator(app)?));
    items.push(Box::new(MenuItem::with_id(
        app,
        "open",
        "Open OpenKeyForge",
        true,
        None::<&str>,
    )?));
    items.push(Box::new(MenuItem::with_id(
        app,
        "quit",
        "Quit",
        true,
        None::<&str>,
    )?));
    let refs: Vec<&dyn IsMenuItem<Wry>> = items.iter().map(|i| i.as_ref()).collect();
    Menu::with_items(app, &refs)
}

fn tooltip(profiles: &Profiles) -> String {
    let name = profiles
        .active
        .as_ref()
        .and_then(|id| profiles.profiles.iter().find(|p| &p.id == id))
        .map(|p| p.name.as_str());
    match name {
        Some(n) => format!("OpenKeyForge: {n}"),
        None => "OpenKeyForge".into(),
    }
}

pub fn show_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// Creates, updates or removes the tray to match `enabled` and `profiles`.
pub fn sync(app: &AppHandle, enabled: bool, profiles: &Profiles) -> Result<(), String> {
    if !enabled {
        app.remove_tray_by_id(TRAY_ID);
        return Ok(());
    }
    let menu = build_menu(app, profiles).map_err(|e| e.to_string())?;
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        tray.set_menu(Some(menu)).map_err(|e| e.to_string())?;
        tray.set_tooltip(Some(tooltip(profiles)))
            .map_err(|e| e.to_string())?;
        return Ok(());
    }
    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .menu(&menu)
        .tooltip(tooltip(profiles))
        // Left click opens the window where the platform reports clicks
        // (Windows, macOS); the menu is on right click there, and on any
        // click under Linux.
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => show_window(app),
            "quit" => app.exit(0),
            id => {
                if let Some(profile) = id.strip_prefix("profile:") {
                    let (app, profile) = (app.clone(), profile.to_string());
                    tauri::async_runtime::spawn_blocking(move || {
                        crate::apply_from_tray(&app, &profile)
                    });
                }
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_window(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder
        .build(app)
        .map(|_| ())
        .map_err(|e| format!("couldn't create the tray icon: {e}"))
}
