use std::ffi::OsString;
use std::num::ParseIntError;

use ch57x::config::KeyboardModel;
use ch57x::parse;
use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
#[clap(version, about = "Program CH57x macro keypads (part of OpenKeyForge)")]
pub struct Options {
    #[command(subcommand)]
    pub command: Command,

    /// HID device path (from `list`) when several keypads are connected
    #[arg(long, global = true)]
    pub path: Option<String>,

    #[clap(flatten)]
    pub devel_options: DevelOptions,
}

#[derive(Args)]
#[clap(next_help_heading = "Internal options (use with caution)")]
pub struct DevelOptions {
    /// Talk to the device through libusb, as the original tool did, instead of hidapi
    #[cfg(feature = "libusb")]
    #[arg(long, global = true)]
    pub libusb: bool,

    #[arg(long, value_parser=hex_or_decimal)]
    pub vendor_id: Option<u16>,

    #[arg(long, value_parser=hex_or_decimal)]
    pub product_id: Option<u16>,

    #[arg(long, value_parser=parse_address)]
    pub address: Option<(u8, u8)>,

    #[arg(long)]
    pub endpoint_address: Option<u8>,

    #[arg(long)]
    pub interface_number: Option<u8>,
}

pub fn hex_or_decimal(s: &str) -> Result<u16, ParseIntError>
{
    if s.to_ascii_lowercase().starts_with("0x") {
        u16::from_str_radix(&s[2..], 16)
    } else {
        #[allow(clippy::from_str_radix_10)] // For consistency with code above
        u16::from_str_radix(s, 10)
    }
}

fn parse_address(s: &str) -> std::result::Result<(u8, u8), nom::error::Error<String>> {
    parse::from_str(parse::address, s)
}

#[derive(Subcommand)]
pub enum Command {
    /// List connected keypads
    List,

    /// Show supported keys and modifiers
    ShowKeys,

    /// Validate key mappings config on stdin
    Validate(ConfigParams),

    /// Upload key mappings from stdin to device
    Upload(UploadParams),

    /// Select LED backlight mode
    Led(LedCommand),

    /// Test LED mode specification without device initialization (hidden command)
    #[command(hide = true)]
    TestLed(TestLedCommand),
}

#[derive(Parser)]
pub struct ConfigParams {
    /// Path to config file to upload.
    /// If not given, read from stdin.
    pub config_path: Option<OsString>,
}

#[derive(Parser)]
pub struct UploadParams {
    #[clap(flatten)]
    pub config: ConfigParams,

    /// Print the USB packets instead of sending them
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Parser)]
pub struct LedCommand {
    #[arg(long)]
    pub model: Option<KeyboardModel>,

    /// Print the USB packets instead of sending them
    #[arg(long)]
    pub dry_run: bool,

    #[arg(num_args=0.., allow_hyphen_values=true)]
    pub args: Vec<String>,
}

#[derive(Parser)]
pub struct TestLedCommand {
    #[arg(long)]
    pub model: KeyboardModel,

    /// LED command arguments (layer and mode)
    #[arg(num_args=0.., allow_hyphen_values=true)]
    pub args: Vec<String>,
}
