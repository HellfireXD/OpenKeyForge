//! Runs the actions the keypad hardware can't: typing text, opening apps,
//! URLs and files, shell commands, and multi-step macros.
//!
//! Each such slot stores a hidden trigger chord on the keypad (see
//! `okf_core::agent::trigger`). How the chord is caught differs by platform:
//!
//! - Windows and macOS register it as a global hotkey, which also keeps it
//!   from reaching the app in front.
//! - Linux has no global hotkey API that works under Wayland, so the HID
//!   thread reads the chord straight from the keypad (needs the udev rule).
//!   The chord still reaches the focused app; with 3+ modifiers and F13-F20
//!   nothing realistically reacts to it.

use std::collections::HashMap;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, bail, ensure, Result};
use enigo::{Direction, Enigo, Key, Keyboard, Settings};
use okf_core::agent::Trigger;
use okf_core::keys::{fold_mods, key_name, parse_accord};
use okf_core::model::{ActionKind, MacroStep};
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tauri_plugin_opener::OpenerExt;

/// Whether this platform/session can run agent actions, and why not.
pub fn availability() -> (bool, Option<&'static str>) {
    if cfg!(target_os = "linux") {
        let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();
        let note = if wayland {
            "Agent chords are read from the keypad (needs the udev rule). Under Wayland, typed text and keystrokes reach X11/XWayland apps only."
        } else {
            "Agent chords are read from the keypad, so the udev rule must be installed."
        };
        (true, Some(note))
    } else if cfg!(target_os = "macos") {
        (true, Some("Typing text and keystrokes needs Accessibility permission for OpenKeyForge (System Settings → Privacy & Security → Accessibility)."))
    } else {
        (true, None)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Fired {
    pub layer: usize,
    pub slot: String,
    /// Milliseconds since the Unix epoch.
    pub at: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentStatus {
    pub enabled: bool,
    pub available: bool,
    pub note: Option<&'static str>,
    pub actions: usize,
    pub last: Option<Fired>,
    /// Chords the OS refused to register (Windows/macOS), if any.
    pub problem: Option<String>,
}

#[derive(Default)]
pub struct Agent {
    enabled: AtomicBool,
    /// Keyed by (left-folded modifier bits, HID key code).
    triggers: Mutex<HashMap<(u8, u8), Trigger>>,
    last: Mutex<Option<Fired>>,
    problem: Mutex<Option<String>>,
    /// One action at a time, so two macros never interleave their keystrokes.
    busy: Arc<Mutex<()>>,
}

fn now_ms() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64() * 1000.0)
        .unwrap_or(0.0)
}

impl Agent {
    pub fn status(&self) -> AgentStatus {
        let (available, note) = availability();
        AgentStatus {
            enabled: self.enabled.load(Ordering::Relaxed),
            available,
            note,
            actions: self.triggers.lock().unwrap().len(),
            last: self.last.lock().unwrap().clone(),
            problem: self.problem.lock().unwrap().clone(),
        }
    }

    pub fn set_triggers(&self, app: &AppHandle, triggers: Vec<Trigger>) {
        *self.triggers.lock().unwrap() = triggers
            .into_iter()
            .map(|t| ((fold_mods(t.mods), t.code), t))
            .collect();
        self.sync_hotkeys(app);
    }

    pub fn set_enabled(&self, app: &AppHandle, on: bool) {
        self.enabled.store(on, Ordering::Relaxed);
        self.sync_hotkeys(app);
    }

    #[cfg(any(windows, target_os = "macos"))]
    fn sync_hotkeys(&self, app: &AppHandle) {
        use tauri_plugin_global_shortcut::GlobalShortcutExt;

        let gs = app.global_shortcut();
        let _ = gs.unregister_all();
        let mut failed = Vec::new();
        if self.enabled.load(Ordering::Relaxed) {
            for &(mods, code) in self.triggers.lock().unwrap().keys() {
                match hotkey::shortcut(mods, code) {
                    Some(s) => {
                        if let Err(e) = gs.register(s) {
                            failed.push(format!("{s}: {e}"));
                        }
                    }
                    None => failed.push(format!("{mods:#x}+{code:#x}: not a trigger chord")),
                }
            }
        }
        *self.problem.lock().unwrap() = (!failed.is_empty()).then(|| {
            format!(
                "Some agent chords couldn't be registered: {}",
                failed.join("; ")
            )
        });
    }

    #[cfg(not(any(windows, target_os = "macos")))]
    fn sync_hotkeys(&self, _app: &AppHandle) {}

    /// A chord arrived (hotkey handler or the keypad watcher).
    pub fn on_chord(&self, app: &AppHandle, mods: u8, code: u8) {
        if !self.enabled.load(Ordering::Relaxed) {
            return;
        }
        let Some(trigger) = self
            .triggers
            .lock()
            .unwrap()
            .get(&(fold_mods(mods), code))
            .cloned()
        else {
            return;
        };
        let fired = Fired {
            layer: trigger.layer,
            slot: trigger.slot.clone(),
            at: now_ms(),
        };
        *self.last.lock().unwrap() = Some(fired.clone());
        let _ = app.emit("agent-fired", &fired);
        log::info!("layer {} {}: agent action", trigger.layer + 1, trigger.slot);
        // On Linux the chord isn't swallowed; give its modifiers time to come
        // up so they don't combine with what the action types.
        let settle = if cfg!(target_os = "linux") { 150 } else { 50 };
        self.run(app, trigger.action.kind, Duration::from_millis(settle));
    }

    /// Runs `kind` on a worker thread after `delay`.
    pub fn run(&self, app: &AppHandle, kind: ActionKind, delay: Duration) {
        let (app, busy) = (app.clone(), self.busy.clone());
        std::thread::spawn(move || {
            std::thread::sleep(delay);
            let _guard = busy.lock().unwrap_or_else(|e| e.into_inner());
            if let Err(e) = run_action(&app, &kind) {
                log::warn!("agent action failed: {e:#}");
                let _ = app.emit("agent-error", format!("{e:#}"));
            }
        });
    }
}

// --- performing actions ----------------------------------------------------------

fn run_action(app: &AppHandle, kind: &ActionKind) -> Result<()> {
    match kind {
        ActionKind::Text { text } => type_text(&mut enigo()?, text),
        ActionKind::Open { target } => open_target(app, target),
        ActionKind::Shell { command } => run_shell(command, false),
        ActionKind::Macro { steps } => {
            let mut keys = None;
            for step in steps {
                let repeat = match step {
                    MacroStep::Wait { .. } => 1,
                    MacroStep::Keys { repeat, .. }
                    | MacroStep::Text { repeat, .. }
                    | MacroStep::Open { repeat, .. }
                    | MacroStep::Shell { repeat, .. } => (*repeat).max(1),
                };
                for _ in 0..repeat {
                    match step {
                        MacroStep::Keys { value, .. } => {
                            let e = match &mut keys {
                                Some(e) => e,
                                None => keys.insert(enigo()?),
                            };
                            for accord in value.split(',').map(str::trim).filter(|s| !s.is_empty())
                            {
                                press_accord(e, accord)?;
                            }
                        }
                        MacroStep::Text { value, .. } => {
                            let e = match &mut keys {
                                Some(e) => e,
                                None => keys.insert(enigo()?),
                            };
                            type_text(e, value)?;
                        }
                        MacroStep::Wait { ms } => {
                            std::thread::sleep(Duration::from_millis(*ms as u64))
                        }
                        MacroStep::Open { value, .. } => open_target(app, value)?,
                        MacroStep::Shell { value, .. } => run_shell(value, true)?,
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
            }
            Ok(())
        }
        _ => bail!("not an agent action"),
    }
}

fn enigo() -> Result<Enigo> {
    Enigo::new(&Settings::default()).map_err(|e| anyhow!("can't simulate input: {e}"))
}

fn type_text(e: &mut Enigo, text: &str) -> Result<()> {
    for (i, line) in text.split('\n').enumerate() {
        if i > 0 {
            e.key(Key::Return, Direction::Click)?;
        }
        if !line.is_empty() {
            e.text(line)?;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    Ok(())
}

/// The enigo key for a HID keyboard usage, or `None` if it can't be sent on
/// every platform. Characters are sent as Unicode so the user's layout
/// doesn't matter.
pub fn enigo_key(code: u8) -> Option<Key> {
    let name = key_name(code)?;
    let f = [
        Key::F1,
        Key::F2,
        Key::F3,
        Key::F4,
        Key::F5,
        Key::F6,
        Key::F7,
        Key::F8,
        Key::F9,
        Key::F10,
        Key::F11,
        Key::F12,
        Key::F13,
        Key::F14,
        Key::F15,
        Key::F16,
        Key::F17,
        Key::F18,
        Key::F19,
        Key::F20,
    ];
    let pad = [
        Key::Numpad0,
        Key::Numpad1,
        Key::Numpad2,
        Key::Numpad3,
        Key::Numpad4,
        Key::Numpad5,
        Key::Numpad6,
        Key::Numpad7,
        Key::Numpad8,
        Key::Numpad9,
    ];
    let uni = |c| Some(Key::Unicode(c));
    match name {
        n if n.chars().count() == 1 => uni(n.chars().next()?),
        "enter" | "numpadenter" => Some(Key::Return),
        "escape" => Some(Key::Escape),
        "backspace" => Some(Key::Backspace),
        "tab" => Some(Key::Tab),
        "space" => Some(Key::Space),
        "minus" => uni('-'),
        "equal" => uni('='),
        "leftbracket" => uni('['),
        "rightbracket" => uni(']'),
        "backslash" => uni('\\'),
        "semicolon" => uni(';'),
        "quote" => uni('\''),
        "grave" => uni('`'),
        "comma" => uni(','),
        "dot" => uni('.'),
        "slash" => uni('/'),
        "capslock" => Some(Key::CapsLock),
        "delete" => Some(Key::Delete),
        "home" => Some(Key::Home),
        "end" => Some(Key::End),
        "pageup" => Some(Key::PageUp),
        "pagedown" => Some(Key::PageDown),
        "right" => Some(Key::RightArrow),
        "left" => Some(Key::LeftArrow),
        "down" => Some(Key::DownArrow),
        "up" => Some(Key::UpArrow),
        "numpadslash" => Some(Key::Divide),
        "numpadasterisk" => Some(Key::Multiply),
        "numpadminus" => Some(Key::Subtract),
        "numpadplus" => Some(Key::Add),
        "numpaddot" => Some(Key::Decimal),
        n => {
            if let Some(d) = n
                .strip_prefix("numpad")
                .and_then(|d| d.parse::<usize>().ok())
            {
                return pad.get(d).copied();
            }
            if let Some(i) = n.strip_prefix('f').and_then(|i| i.parse::<usize>().ok()) {
                return f.get(i.checked_sub(1)?).copied();
            }
            None
        }
    }
}

/// Every accord in a macro key step can be sent, or the first that can't.
pub fn check_sendable(accords: &str) -> Result<()> {
    for a in accords.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        if let Some(code) = parse_accord(a)?.code {
            ensure!(
                enigo_key(code).is_some(),
                "{a:?} can't be sent by the agent"
            );
        }
    }
    Ok(())
}

const MODIFIER_KEYS: [(u8, Key); 8] = [
    (0x01, Key::Control),
    (0x02, Key::Shift),
    (0x04, Key::Alt),
    (0x08, Key::Meta),
    (0x10, Key::RControl),
    (0x20, Key::RShift),
    (0x40, Key::Alt),
    (0x80, Key::Meta),
];

fn press_accord(e: &mut Enigo, accord: &str) -> Result<()> {
    let a = parse_accord(accord)?;
    let key = match a.code {
        Some(code) => {
            Some(enigo_key(code).ok_or_else(|| anyhow!("{accord:?} can't be sent by the agent"))?)
        }
        None => None,
    };
    let mods: Vec<Key> = MODIFIER_KEYS
        .iter()
        .filter(|(bit, _)| a.mods & bit != 0)
        .map(|(_, k)| *k)
        .collect();
    for m in &mods {
        e.key(*m, Direction::Press)?;
    }
    let clicked = key.map_or(Ok(()), |k| e.key(k, Direction::Click));
    for m in mods.iter().rev() {
        e.key(*m, Direction::Release)?;
    }
    clicked?;
    std::thread::sleep(Duration::from_millis(8));
    Ok(())
}

fn expand_home(target: &str) -> String {
    match target.strip_prefix('~') {
        Some(rest) if rest.is_empty() || rest.starts_with('/') || rest.starts_with('\\') => {
            let home = std::env::var("HOME")
                .or_else(|_| std::env::var("USERPROFILE"))
                .unwrap_or_default();
            format!("{home}{rest}")
        }
        _ => target.to_string(),
    }
}

fn looks_like_path(t: &str) -> bool {
    let b = t.as_bytes();
    t.starts_with('/')
        || t.starts_with('\\')
        || t.starts_with('~')
        || (b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':')
        || std::path::Path::new(t).exists()
}

fn open_target(app: &AppHandle, target: &str) -> Result<()> {
    let t = target.trim();
    // Openers read a leading "-" as one of their own options.
    ensure!(
        !t.is_empty() && !t.starts_with('-'),
        "refusing to open {t:?}"
    );
    if t.contains("://") || t.starts_with("mailto:") {
        app.opener().open_url(t, None::<&str>)?;
    } else if looks_like_path(t) {
        app.opener().open_path(expand_home(t), None::<&str>)?;
    } else if cfg!(target_os = "macos") {
        // An app name such as "Safari".
        Command::new("open").args(["-a", t]).spawn()?;
    } else {
        // A program on PATH, such as "code" or "notepad".
        host_command(t).spawn()?;
    }
    Ok(())
}

/// A command that runs outside the Flatpak sandbox when there is one.
fn host_command(program: &str) -> Command {
    if cfg!(target_os = "linux") && std::env::var_os("FLATPAK_ID").is_some() {
        let mut c = Command::new("flatpak-spawn");
        c.args(["--host", program]);
        c
    } else {
        Command::new(program)
    }
}

/// The shell the shell action uses on this platform, for the editor to show.
pub fn shell_name() -> &'static str {
    if cfg!(windows) {
        "powershell -NoProfile -Command"
    } else if cfg!(target_os = "macos") {
        "zsh -lc"
    } else {
        "sh -lc"
    }
}

fn run_shell(command: &str, wait: bool) -> Result<()> {
    #[cfg(windows)]
    let mut c = {
        use std::os::windows::process::CommandExt as _;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let mut c = Command::new("powershell.exe");
        c.args(["-NoProfile", "-NonInteractive", "-Command", command])
            .creation_flags(CREATE_NO_WINDOW);
        c
    };
    #[cfg(target_os = "macos")]
    let mut c = {
        let mut c = Command::new("/bin/zsh");
        c.args(["-lc", command]);
        c
    };
    #[cfg(not(any(windows, target_os = "macos")))]
    let mut c = {
        let mut c = host_command("/bin/sh");
        c.args(["-lc", command]);
        c
    };
    let mut child = c
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    if wait {
        child.wait()?;
    } else {
        // Reap it in the background so it doesn't linger as a zombie.
        std::thread::spawn(move || child.wait());
    }
    Ok(())
}

#[cfg(any(windows, target_os = "macos"))]
pub mod hotkey {
    use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut};

    const F_KEYS: [(u8, Code); 8] = [
        (0x68, Code::F13),
        (0x69, Code::F14),
        (0x6A, Code::F15),
        (0x6B, Code::F16),
        (0x6C, Code::F17),
        (0x6D, Code::F18),
        (0x6E, Code::F19),
        (0x6F, Code::F20),
    ];

    pub fn shortcut(mods: u8, code: u8) -> Option<Shortcut> {
        let key = F_KEYS.iter().find(|(c, _)| *c == code)?.1;
        let mut m = Modifiers::empty();
        for (bit, flag) in [
            (1, Modifiers::CONTROL),
            (2, Modifiers::SHIFT),
            (4, Modifiers::ALT),
            (8, Modifiers::SUPER),
        ] {
            if mods & bit != 0 {
                m |= flag;
            }
        }
        Some(Shortcut::new(Some(m), key))
    }

    /// The (modifier bits, HID code) a registered shortcut stands for.
    pub fn chord(s: &Shortcut) -> Option<(u8, u8)> {
        let code = F_KEYS.iter().find(|(_, k)| *k == s.key)?.0;
        let mut mods = 0;
        for (bit, flag) in [
            (1, Modifiers::CONTROL),
            (2, Modifiers::SHIFT),
            (4, Modifiers::ALT),
            (8, Modifiers::SUPER),
        ] {
            if s.mods.contains(flag) {
                mods |= bit;
            }
        }
        Some((mods, code))
    }
}
