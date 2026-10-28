# Adding a device family

OpenKeyForge has three layers. Adding hardware touches only the bottom two, plus one line in
the app:

1. **Engine** (`crates/engines/<family>`): the device's protocol and transport. Ideally an
   existing tool, vendored or wrapped. It must not depend on `openkeyforge-core`, so it stays
   usable (and shareable upstream) on its own. `crates/engines/ch57x` is ch57x-keyboard-tool.
2. **Driver** (`crates/drivers/<family>`): implements
   [`okf_core::DeviceDriver`](../crates/core/src/driver.rs) and translates the standard keymap
   into the engine's types. `crates/drivers/ch57x` is the reference.
3. **Register it** in `registry()` in [`src-tauri/src/lib.rs`](../src-tauri/src/lib.rs), and
   add its USB IDs to [`packaging/linux/70-openkeyforge.rules`](../packaging/linux/70-openkeyforge.rules).

The UI needs no changes: it reads models and capabilities from the driver.

## The keymap a driver receives

`okf_core::Mapping` (see [`crates/core/src/model.rs`](../crates/core/src/model.rs)) holds
`driver`, `model`, `rows`, `columns`, `knobs` and `layers`. Each layer maps slot names to
actions:

- slots: `b0`…`b<n>` for buttons, row by row; `k<i>.ccw`, `k<i>.press`, `k<i>.cw` for knobs.
- actions: `keys` (accords in the [`okf_core::keys`](../crates/core/src/keys.rs) vocabulary,
  e.g. `ctrl-shift-a`), `media` (HID Consumer usage), `mouse`, and `raw` (a driver-specific
  expression).

Drivers never see software actions (`text`, `open`, `shell`, `macro`). `prepare()` replaces
each with a hidden trigger chord as a plain `keys` action before calling `encode`, and the
app's agent handles the rest. A device that can store a keystroke supports agent actions for
free.

## What to implement

| Method | Purpose |
|---|---|
| `id`, `name` | Stable id stored in keymaps (`"via"`), and a display name |
| `models` | Each model's `Capabilities`: action types, limits, layout limits, lighting, `read_keymap`, `packet_preview` |
| `starter` | The keymap a new user of a model starts from |
| `detect` | Connected devices (hidapi path, IDs, candidate models) |
| `input_interfaces` | hidapi paths the device types into, for the live key test |
| `check_layout` | Whole-keymap validation (layout fits the model, lighting values) |
| `check_action` | One slot's action; errors show on that key in the editor |
| `encode` | Keymap → engine payload (+ optional human-readable preview) |
| `write` | Payload → device |
| `read_keymap` | Device → keymap, only if `Capabilities::read_keymap` |

Use `okf_core::driver::slot_errors` and `prepare` in your tests, as the CH57x driver does.
They run the same checks and lowering the app runs.

## Hardware access rules

- Use the shared `okf_core::hidapi`, so every driver links one hidapi build.
- All hidapi calls run on the app's single HID thread (macOS requires it). Don't spawn
  threads that touch hidapi.
- Linux: add udev rules **by vendor and product ID** (or by a serial-number marker, as Vial
  does). Never add a catch-all `hidraw*` rule.
