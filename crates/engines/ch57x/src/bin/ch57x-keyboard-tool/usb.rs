//! The original tool's libusb transport (kriomant/ch57x-keyboard-tool),
//! kept for keypads whose vendor interface hidapi can't reach.

use anyhow::{Result, anyhow, ensure};
use ch57x::config::{KeyboardModel, DeviceInfo};
use indoc::indoc;
use itertools::Itertools;
use log::debug;
use rusb::{Context, Device, DeviceDescriptor, DeviceHandle, TransferType};

use anyhow::Context as _;
use rusb::UsbContext as _;

use crate::options::DevelOptions;

fn find_interface_and_endpoint(
    device: &Device<Context>,
    interface_num: Option<u8>,
    endpoint_addr: u8,
) -> Result<(u8, u8)> {
    let conf_desc = device
        .config_descriptor(0)
        .context("get config #0 descriptor")?;

    // Get the numbers of interfaces to explore
    let interface_nums = match interface_num {
        Some(iface_num) => vec![iface_num],
        None => conf_desc.interfaces().map(|iface| iface.number()).collect(),
    };

    for iface_num in interface_nums {
        debug!("Probing interface {iface_num}");

        // Look for an interface with the given number
        let intf = conf_desc
            .interfaces()
            .find(|iface| iface_num == iface.number())
            .ok_or_else(|| {
                anyhow!(
                    "interface #{} not found, interface numbers:\n{:#?}",
                    iface_num,
                    conf_desc.interfaces().map(|i| i.number()).format(", ")
                )
            })?;

        // Check that it's a HID device
        let intf_desc = intf.descriptors().exactly_one().map_err(|_| {
            anyhow!(
                "only one interface descriptor is expected, got:\n{:#?}",
                intf.descriptors().format("\n")
            )
        })?;

        // Look for suitable endpoints
        if let Some(endpt_desc) = intf_desc.endpoint_descriptors().find(|ep| {
            ep.transfer_type() == TransferType::Interrupt && ep.address() == endpoint_addr
        }) {
            debug!("Found endpoint {endpt_desc:?}");
            if intf_desc.class_code() == 0x03
                && intf_desc.sub_class_code() == 0x00
                && intf_desc.protocol_code() == 0x00
            {
                return Ok((iface_num, endpt_desc.address()));
            } else {
                debug!("unexpected interface parameters: {:#?}", intf_desc);
            }
        }
    }

    Err(anyhow!("No valid interface/endpoint combination found!"))
}

pub fn send_to_device(handle: &DeviceHandle<Context>, endpoint: u8, output: &[u8]) -> Result<()> {
    use std::time::Duration;
    use log::debug;

    const DEFAULT_TIMEOUT: Duration = Duration::from_millis(100);

    // Process output buffer in 64-byte chunks
    for chunk in output.chunks(64) {
        debug!("send: {:02x?}", chunk);
        let written = handle.write_interrupt(endpoint, chunk, DEFAULT_TIMEOUT)?;
        ensure!(written == chunk.len(), "not all data written");
    }
    Ok(())
}

pub fn open_device(
    devel_options: &DevelOptions,
    configured_model: Option<KeyboardModel>,
) -> Result<(DeviceHandle<Context>, u8, KeyboardModel)> {
    // Find USB device based on the configured model or product id
    let (device, desc, info) = find_device(devel_options, configured_model).context("find USB device")?;

    ensure!(
        desc.num_configurations() == 1,
        "only one device configuration is expected"
    );

    // Find correct endpoint
    let (intf_num, endpt_addr) = find_interface_and_endpoint(
        &device,
        devel_options.interface_number,
        devel_options.endpoint_address.unwrap_or(info.preferred_endpoint),
    )?;

    // Open device.
    let mut handle = device.open().context("open USB device")?;
    let _ = handle.set_auto_detach_kernel_driver(true);
    handle
        .claim_interface(intf_num)
        .context("claim interface")?;

    // Initialize device.
    send_to_device(&handle, endpt_addr, &[0u8; 64])?;

    Ok((handle, endpt_addr, info.model))
}

fn find_device(
    devel_options: &DevelOptions,
    configured_model: Option<KeyboardModel>,
) -> Result<(Device<Context>, DeviceDescriptor, DeviceInfo)> {
    let options = vec![
        #[cfg(windows)] rusb::UsbOption::use_usbdk(),
    ];
    let usb_context = rusb::Context::with_options(&options)?;

    let mut found = vec![];
    for device in usb_context.devices().context("get USB device list")?.iter() {
        let desc = device.device_descriptor().context("get USB device info")?;
        debug!(
            "Bus {:03} Device {:03} ID {:04x}:{:04x}",
            device.bus_number(),
            device.address(),
            desc.vendor_id(),
            desc.product_id()
        );

        let device_address = (device.bus_number(), device.address());
        if let Some(address) = devel_options.address && device_address != address {
            continue;
        }

        if let Some(vid) = devel_options.vendor_id {
            if desc.vendor_id() != vid {
                continue;
            }
        }

        if let Some(pid) = devel_options.product_id {
            if desc.product_id() != pid {
                continue;
            }
        }

        let models = if let Some(model) = configured_model {
            let supported_pairs = model.supported_vid_pid();
            let device_vid = desc.vendor_id();
            let device_pid = desc.product_id();

            let supported = match (devel_options.vendor_id, devel_options.product_id) {
                (Some(_), Some(_)) => {
                    true
                },
                (Some(_), None) => {
                    // Vendor ID is specified by user, accept it, but check that Product ID is among
                    // supported by this model.
                    supported_pairs.iter().any(|(_, pid)| *pid == device_pid)
                },
                (None, Some(_)) => {
                    // Product ID is specified by user, accept it, but check that Vendor ID is among
                    // supported by this model.
                    supported_pairs.iter().any(|(vid, _)| *vid == device_vid)
                }
                (None, None) => {
                    supported_pairs.contains(&(device_vid, device_pid))
                }
            };

            if !supported {
                continue;
            }

            vec![model.device_info(device_vid, device_pid).ok_or_else(|| anyhow!(
                "No known preferred endpoint for device {:04x}:{:04x}. \
                 Use --endpoint-address to specify it.",
                device_vid,
                device_pid
            ))?]
        } else {
            let models = KeyboardModel::from_vid_pid(desc.vendor_id(), desc.product_id());
            if models.is_empty() {
                continue;
            }
            models
        };

        found.push((device, desc, models));
    }

    match found.len() {
        0 => Err(anyhow!(
            "CH57x keyboard device not found. Use --vendor-id and/or --product-id to override."
        )),
        1 => {
            let (device, desc, models) = found.pop().unwrap();
            if models.len() == 1 {
                return Ok((device, desc, models[0]));
            }

            Err(anyhow!(indoc! {"
                Found device, but there are several different models which can use such product ID.
                You need to select model and:
                * write it to configuration file ('model' field) for 'upload' command
                * or provide `--model` argument for 'led' command
                Possible values inferred from product ID: {}
                If you are not sure which model to use, just try each and find one which works.
            "}, models.iter().map(|info| format!("{}", info.model)).join("\n")))
        }
        _ => {
            Err(anyhow!(indoc! {"
                Several compatible devices are found.
                Unfortunately, this model of keyboard doesn't have serial number.
                So specify USB address using --address option.

                Addresses:
                {}
            "}, found.iter()
                     .map(|(device, _, _)| format!("{}:{}", device.bus_number(), device.address()))
                     .join("\n")))
        }
    }
}

