# OpenKeyForge

A cross-platform, vendor-agnostic, open-source desktop forge for configuring, mapping and
flashing **macro pads, key decks and rotary encoders**, on **Linux, Windows and macOS**.

The programmable macro pad world is fragmented. Enthusiast boards speak QMK/VIA/Vial, budget
OEM pads (like the WCH **CH57x** 12-key, 2-knob pads) need a closed, Windows-only vendor tool,
and boutique hardware ships its own app. OpenKeyForge is one configurator for all of them,
in the spirit of [OpenRGB](https://openrgb.org): a modern visual editor on top of pluggable
device drivers.

> OpenKeyForge is an independent, community-driven open-source project and is not affiliated
> with, sponsored by, or endorsed by KeyForge, Inc., keyforge.tech, or any hardware
> manufacturer. All product names, logos, and brands are property of their respective owners.

## Architecture

```text
 ┌──────────────────────────────────────────────────────────────┐
 │  UI  (src/: Svelte 5 + TypeScript; src-tauri/: Tauri 2 app)  │
 │  Visual grid, key/knob recorder, layer tabs, lighting,       │
 │  live key test, agent for actions hardware can't store       │
 └──────────────────────────────┬───────────────────────────────┘
                                │  standard keymap (okf_core::Mapping)
 ┌──────────────────────────────▼───────────────────────────────┐
 │  Driver layer                                                │
 │  crates/core: keymap model, key vocabulary, DeviceDriver     │
 │               trait + capabilities, driver registry          │
 │  crates/drivers/<family>: translates the keymap into one     │
 │               engine's language                              │
 └──────────────────────────────┬───────────────────────────────┘
                                │  engine-native config
 ┌──────────────────────────────▼───────────────────────────────┐
 │  Programmer engines  (crates/engines/<family>)               │
 │  Protocol + transport only; know nothing about OpenKeyForge  │
 └──────────────────────────────────────────────────────────────┘
```

Every driver implements one trait (`crates/core/src/driver.rs`):

```rust
pub trait DeviceDriver: Send + Sync {
    fn id(&self) -> &'static str;                         // "ch57x"
    fn models(&self) -> Vec<ModelInfo>;                   // each with its Capabilities
    fn starter(&self, model: &str) -> Result<Mapping>;
    fn detect(&self, hid: &HidApi) -> Vec<DetectedDevice>;
    fn check_layout(&self, keymap: &Mapping) -> Result<()>;
    fn check_action(&self, keymap: &Mapping, layer: usize, slot: Slot, action: &ActionKind) -> Result<()>;
    fn encode(&self, keymap: &Mapping) -> Result<Encoded>; // keymap → engine language
    fn write(&self, hid: &HidApi, device: &DetectedDevice, encoded: &Encoded) -> Result<()>;
    fn read_keymap(&self, hid: &HidApi, device: &DetectedDevice) -> Result<Mapping>; // if capable
    // …
}
```

Devices differ a lot, so the trait reports **capabilities** rather than assuming them: which
action types the device stores, sequence length and delay limits, layout limits, lighting
modes, and whether it can report its keymap (`read_keymap`). The editor shows only what the
selected device supports. See [docs/DRIVERS.md](docs/DRIVERS.md) to add a device family.

## Device support

### Phase 1: CH57x / WinChipHead pads (done)

The cheap 12-key, 2-knob pads (`MQ-AX18-K12-VT2-RGB` and relatives) and their siblings,
programmed directly over USB HID. No vendor software, no driver installation.

| Model | USB IDs | Notes |
|---|---|---|
| `ch57x-1` | `1189:8840`, `1189:8842`, `1189:8850`, `514c:8851` | Up to 15 keys + 3 knobs, or 12 keys + 4 knobs; 18-step sequences with delay; per-layer LEDs |
| `ch57x-2` | `1189:8890` | A different protocol: up to 12 keys + 3 knobs, 5-step sequences, no delay |
| `ch57x-3` | `514c:8850` | 4×4 layouts, no delay, no mouse drag |

Every model: 3 hardware layers; each knob has `ccw`, `press` and `cw`; keystrokes, media keys
(any HID Consumer code), mouse click/scroll/move/drag, and ch57x-keyboard-tool expressions.
These pads **can't report their keymap**, so every upload rewrites every slot.

### Phase 2: VIA / Vial (planned)

QMK boards with VIA or Vial over the RawHID interface (usage page `0xFF60`, usage `0x61`).
Notes from the protocol, for whoever picks this up:

- `0x01` get protocol version, `0x04`/`0x05` get/set a keycode (layer, row, column), `0x11`/`0x12`
  get layer count and a keymap buffer, so these boards **can** report their keymap.
- `0x05` writes straight to EEPROM; there is no separate save step. (`0x07`–`0x09` are
  lighting/custom values.)
- VIA boards don't describe their own layout: VIA looks up a definition JSON by VID/PID.
  **Vial embeds that definition in the firmware**, so a generic driver should start with
  Vial and accept user-supplied VIA definitions.
- Implementing the protocol needs no code from VIA/Vial. If code is ever ported from
  vial-gui, check its exact GPL version first.

### Phase 3: boutique and proprietary hardware

Pluggable drivers for stream decks and boutique pads, one crate each, with no changes to the
UI or the other drivers.

## Features

- **Visual editor** for every layer, key and knob. Drag keys to swap them, copy and paste
  slots, set each layer's lighting.
- **Key recorder**: press a shortcut and it's captured.
- **Live key test**: press a key on the device and the editor highlights the slot it fired.
- **Profiles** spanning several devices, switchable from the app or an optional **tray icon**.
- **Agent actions** for what hardware can't store: type any Unicode text, open apps, URLs and
  files, run shell commands, and macros with waits and repeats. Works with every driver.
- **Import** ch57x-keyboard-tool YAML configs and macropad-studio mappings; export keymaps as
  JSON.
- English and Korean UI; light and dark mode.
- The **`ch57x-keyboard-tool` CLI**, compatible with the original's commands and configs.

## Install

Download from [Releases](https://github.com/HellfireXD/OpenKeyForge/releases):

| System | Package |
|---|---|
| Windows (x64, ARM64) | `OpenKeyForge_…_x64-setup.exe`, or the portable `.exe` |
| macOS (Intel + Apple silicon) | `OpenKeyForge_…_universal.dmg` |
| Debian / Ubuntu | `sudo apt install ./OpenKeyForge_…_amd64.deb` |
| Fedora / RHEL / openSUSE | `sudo dnf install ./OpenKeyForge-…x86_64.rpm` |
| Arch | `sudo pacman -U openkeyforge-bin-…pkg.tar.zst` |
| Any Linux, portable | `OpenKeyForge_…_amd64.AppImage` |

### Linux permissions

Linux only lets root open raw HID devices. The **.deb, .rpm and Arch** packages install
[`70-openkeyforge.rules`](packaging/linux/70-openkeyforge.rules), which grants access to the
logged-in user for **supported devices only** (`uaccess`, matched by vendor and product ID).
For the **AppImage**:

```sh
sudo curl -L -o /etc/udev/rules.d/70-openkeyforge.rules \
  https://raw.githubusercontent.com/HellfireXD/OpenKeyForge/main/packaging/linux/70-openkeyforge.rules
sudo udevadm control --reload-rules && sudo udevadm trigger
```

Then replug the device. Don't replace it with a catch-all `hidraw*` rule: that would give
every program in your session raw access to every HID device, keyboards and security keys
included.

### Windows and macOS

No drivers needed: devices are reached through the system HID stack, and once programmed they
work as ordinary keyboards on any computer, with or without OpenKeyForge.

## Profiles and the tray

A **profile** ("Halo", "Adobe") holds one keymap per device. **Apply profile** writes each of
its keymaps to the matching attached device; devices the profile doesn't include keep what
they have. With two pads, "Halo" can hold a keymap for each, and switching to "Adobe" rewrites
whichever pads "Adobe" has keymaps for.

Each keymap in a profile is bound to a device by driver, model and USB ID, then by serial
number if the device has one. CH57x pads report no serial number, so identical pads are told
apart by their order among attached pads; if two identical pads swap after you move them to
other ports, rebind them in the profile bar. A keymap can also be left unbound ("any matching
device").

Agent chords are allocated across devices, so two pads never send the same chord for
different actions, even when one pad still holds another profile's keymap.

**System tray** (optional, ⋯ menu → *Keep running in system tray*): a tray icon lists your
profiles; choosing one applies it. While it's on, closing the window keeps OpenKeyForge (and
the agent) running. On Linux the tray needs a StatusNotifier host (KDE and most desktops;
GNOME with the AppIndicator extension); the .deb, .rpm and Arch packages pull in
libayatana-appindicator, and the AppImage bundles it.

## The agent

For software actions (text, open, shell, macro) the device stores a hidden **trigger chord**,
3–4 modifiers plus F13–F20 (e.g. Ctrl+Alt+Shift+F16), and the agent performs the real action
when that chord arrives. Turn it on with **Agent** in the toolbar. Agent slots work only while
OpenKeyForge is running.

| | How the chord is caught | Notes |
|---|---|---|
| **Windows** | Global hotkey (swallowed) | Up to 32 agent slots (Ctrl+Alt+Shift+Win is reserved for the Office key). Shell actions run in PowerShell. |
| **macOS** | Global hotkey (swallowed) | Up to 40 slots. Typing needs **Accessibility** permission. Shell actions run in `zsh -lc`. |
| **Linux** | Read straight from the device | Works on X11 and Wayland, but needs the udev rule, and the chord also reaches the focused app (harmless). Under Wayland, typed text reaches X11/XWayland apps only. Shell actions run in `sh -lc`. |

**Security:** a keymap's shell and open actions run as you. Importing a keymap that contains
any shows each one and asks first. Only use keymaps you wrote or trust.

## What has been tested

- The CH57x encoder is byte-for-byte ch57x-keyboard-tool's (its test vectors are kept and
  pass), the driver's key vocabulary encodes exactly like the engine's own parser for every
  key, and keymaps reproduce macropad-studio's packets.
- Builds, type checks and tests run in Docker and in GitHub Actions on Linux, Windows and
  macOS; the .deb, .rpm, Arch and AppImage packages build and install cleanly in
  containers of their target distros.
- **Not yet tested on real hardware.** CH57x uploads over hidapi follow macropad-studio, which
  was verified on `1189:8840` under macOS. `ch57x-2`/`ch57x-3` pads were only ever driven
  over libusb by the original tool, so hidapi uploads for them are untested.

## Development

All tooling runs in a container; only Docker is needed on the host.

```sh
docker build -f docker/dev.Dockerfile -t okf-dev docker
docker run -d --name okf-dev -v "$PWD:/work" -v okf-target:/work/target \
  -v okf-cargo:/usr/local/cargo/registry -v okf-npm:/work/node_modules okf-dev sleep infinity

docker exec okf-dev npm ci
docker exec okf-dev npm run check                    # svelte-check
docker exec okf-dev npm test                         # frontend tests
docker exec okf-dev cargo test --workspace --all-features
docker exec okf-dev cargo clippy --workspace --all-features --all-targets -- -D warnings
docker exec okf-dev cargo check -p openkeyforge --target x86_64-pc-windows-msvc
docker exec okf-dev npm run tauri build              # .deb, .rpm, AppImage
```

| Path | Layer |
|---|---|
| `src/`, `src-tauri/` | UI: Svelte editor and the Tauri app (commands, HID thread, agent, storage) |
| `crates/core/` | Driver layer: keymap model, keys, agent lowering, `DeviceDriver` trait, registry, input decoding |
| `crates/drivers/ch57x/` | Driver layer: CH57x translator and YAML import |
| `crates/engines/ch57x/` | Engine: ch57x-keyboard-tool's protocol (vendored), hidapi transport, CLI |
| `packaging/` | udev rule, Arch PKGBUILD |
| `.github/workflows/` | `test.yml` on every push/PR; `build.yml` (run by hand) builds a draft release |

### Releasing

1. Bump `version` in `package.json`.
2. Run **Build and Release** from the Actions tab.
3. Review the draft release and publish it.

## License

[GPL-3.0-or-later](LICENSE). The vendored ch57x-keyboard-tool engine stays MIT, and the
editor design comes from macropad-studio (MIT); see [NOTICE.md](NOTICE.md).
