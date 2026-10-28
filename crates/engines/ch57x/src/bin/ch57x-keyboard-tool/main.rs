mod options;
// Upstream's libusb code, unchanged.
#[cfg(feature = "libusb")]
#[allow(unused_mut, clippy::collapsible_if)]
mod usb;

use std::io::{BufReader, Read, StdinLock};

use ch57x::config::{Config, KeyboardModel};
use ch57x::keyboard::{Key, KnobAction, MediaCode, Modifier, WellKnownCode};
use ch57x::{create_driver, device, hex_lines};
use crate::options::{Command, ConfigParams, LedCommand, Options, TestLedCommand, UploadParams};

use anyhow::{Context as _, Result, anyhow, bail};
use clap::Parser as _;
use itertools::Itertools;
use strum::EnumMessage as _;
use strum::IntoEnumIterator as _;

fn main() -> Result<()> {
    env_logger::init();
    let options = Options::parse();

    match options.command {
        Command::List => {
            let api = hidapi::HidApi::new().context("initialise hidapi")?;
            let found = device::list(&api);
            if found.is_empty() {
                println!("No CH57x keypad found.");
            }
            for k in found {
                println!("{}  {:?}  models: {}  path={}", k.id(), k.product, k.models.iter().join("/"), k.path);
            }
        }

        Command::ShowKeys => {
            println!("Modifiers: ");
            for m in Modifier::iter() {
                println!(" - {}", m.get_serializations().iter().join(" / "));
            }

            println!();
            println!("Keys:");
            for c in WellKnownCode::iter() {
                println!(" - {c}");
            }

            println!();
            println!("Custom key syntax (use decimal code): <110>");

            println!();
            println!("Media keys:");
            for c in MediaCode::iter() {
                println!(" - {}", c.get_serializations().iter().join(" / "));
            }
            println!("Any HID Consumer usage: consumer(0x1b6)");

            println!();
            println!("Mouse actions:");
            println!(" - wheel(-100)");
            println!(" - click(left+right)");
            println!(" - move(5,0)");
            println!(" - drag(left+right,0,5)");
        }

        Command::Validate(params) => {
            let config: Config = load_config(&params)
                .context("load mapping config")?;
            let _ = config.render().context("render mappings config")?;
            println!("config is valid 👌")
        }

        Command::Upload(UploadParams { config, dry_run }) => {
            let config: Config = load_config(&config)
                .context("load mapping config")?;

            if dry_run {
                let model = config.model.unwrap_or_else(|| {
                    eprintln!("⚠️ Configuration file does not specify `model`; assuming ch57x-1.");
                    KeyboardModel::Ch57x_1
                });
                for line in hex_lines(&render(config, model)?) {
                    println!("{line}");
                }
                return Ok(());
            }

            #[cfg(feature = "libusb")]
            if options.devel_options.libusb {
                let configured_model = config.model;
                let (handle, endpoint, model) = usb::open_device(&options.devel_options, configured_model)?;
                return usb::send_to_device(&handle, endpoint, &render(config, model)?);
            }

            let api = hidapi::HidApi::new().context("initialise hidapi")?;
            let keypad = device::pick(&api, options.path.as_deref())?;
            let model = match config.model {
                Some(model) => model,
                None => {
                    let model = single_model(&keypad)?;
                    eprintln!(
                        "⚠️ Configuration file does not specify `model`. This field will be required in a future version. Add `model: {model}`."
                    );
                    model
                }
            };
            let output = render(config, model)?;
            device::send(&api, &keypad.path, &output)?;
            println!("uploaded {} packets", output.len() / 64);
        }

        Command::Led(LedCommand { model, dry_run, mut args }) => {
            args.insert(0, "led".to_string());

            #[cfg(feature = "libusb")]
            if options.devel_options.libusb {
                let (handle, endpoint, model) = usb::open_device(&options.devel_options, model)?;
                // TODO: fix this dirty hack
                let mut keyboard = create_driver(model, 0, 0)?;
                let mut output = Vec::new();
                keyboard.set_led(&args, &mut output)?;
                return usb::send_to_device(&handle, endpoint, &output);
            }

            let led = |model| -> Result<Vec<u8>> {
                // TODO: fix this dirty hack
                let mut keyboard = create_driver(model, 0, 0)?;
                let mut output = Vec::new();
                keyboard.set_led(&args, &mut output)?;
                Ok(output)
            };
            if dry_run {
                for line in hex_lines(&led(model.unwrap_or(KeyboardModel::Ch57x_1))?) {
                    println!("{line}");
                }
                return Ok(());
            }
            let api = hidapi::HidApi::new().context("initialise hidapi")?;
            let keypad = device::pick(&api, options.path.as_deref())?;
            let model = match model {
                Some(model) => model,
                None => single_model(&keypad)?,
            };
            device::send(&api, &keypad.path, &led(model)?)?;
            println!("LED mode set");
        }

        Command::TestLed(TestLedCommand { model, mut args }) => {
            // Create driver without USB device initialization
            let mut keyboard = create_driver(model, 0, 0)?;
            let mut output = Vec::new();

            // Validate LED arguments
            args.insert(0, "test-led".to_string());
            keyboard.set_led(&args, &mut output)?;

            println!("Generated {} bytes of data", output.len());
        }
    }

    Ok(())
}

fn single_model(keypad: &device::Keypad) -> Result<KeyboardModel> {
    match keypad.models.as_slice() {
        [model] => Ok(*model),
        models => bail!(
            "Found device {}, but there are several different models which can use such product ID.\n\
             Write one to the configuration file ('model' field) or pass `--model`.\n\
             Possible values: {}\n\
             If you are not sure which model to use, just try each and find one which works.",
            keypad.id(),
            models.iter().join(", ")
        ),
    }
}

/// Packets for every configured button and knob, in the original tool's order.
fn render(config: Config, model: KeyboardModel) -> Result<Vec<u8>> {
    let (buttons, knobs) = (config.rows * config.columns, config.knobs);
    let layers = config.render().context("render mapping config")?;
    let keyboard = create_driver(model, buttons, knobs)?;

    let mut output = Vec::new();

    // Apply keyboard mapping.
    for (layer_idx, layer) in layers.iter().enumerate() {
        for (button_idx, macro_) in layer.buttons.iter().enumerate() {
            if let Some(macro_) = macro_ {
                keyboard.bind_key(layer_idx as u8, Key::Button(button_idx as u8), macro_, &mut output)
                    .context("bind key")?;
            }
        }

        for (knob_idx, knob) in layer.knobs.iter().enumerate() {
            if let Some(macro_) = &knob.ccw {
                keyboard.bind_key(layer_idx as u8, Key::Knob(knob_idx as u8, KnobAction::RotateCCW), macro_, &mut output)?;
            }
            if let Some(macro_) = &knob.press {
                keyboard.bind_key(layer_idx as u8, Key::Knob(knob_idx as u8, KnobAction::Press), macro_, &mut output)?;
            }
            if let Some(macro_) = &knob.cw {
                keyboard.bind_key(layer_idx as u8, Key::Knob(knob_idx as u8, KnobAction::RotateCW), macro_, &mut output)?;
            }
        }
    }
    Ok(output)
}

fn open_config(params: &ConfigParams) -> Result<Box<dyn Read>> {
    Ok(match &params.config_path {
        Some(path) => {
            let file = std::fs::File::open(path).context("open config file")?;
            Box::new(BufReader::new(file))
        }
        None => {
            let stdin: BufReader<StdinLock<'static>> = BufReader::new(std::io::stdin().lock());
            Box::new(stdin)
        }
    })
}

fn load_config(params: &ConfigParams) -> Result<Config> {
    // Load and validate mapping.
    serde_yaml::from_reader(open_config(params)?).map_err(|e| anyhow!(e))
}
