#![no_main]
#![no_std]

extern crate alloc;

mod error;
mod hooks;
mod patcher;
mod utils;

use alloc::string::{String, ToString};
use alloc::{format, vec};
use core::slice;

use log::{error, info};
use uefi::proto::device_path::build::DevicePathBuilder;
use uefi::proto::device_path::build::media::FilePath;
use uefi::proto::loaded_image::LoadedImage;
use uefi::{CStr16, Status, boot, helpers, proto};
use windows_sys::w;

use crate::error::{UefiErrorMessage, UefiResultConvertable};
use crate::hooks::{bootmgfw, efi};
use crate::utils::countdown;

fn entrypoint() -> uefi::Result<(), String> {
    helpers::init().map_err(|e| e.convert("Cannot initialize helpers"))?;
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
    efi::patch_system_table();

    let bootmgfw_image = boot::open_protocol_exclusive::<LoadedImage>(bootmgfw_handle)
        .convert("Cannot open protocol of bootmgfw_old.efi")?;
    let (bootmgfw_buffer, bootmgfw_size) = bootmgfw_image.info();
    let bootmgfw_buffer =
        unsafe { slice::from_raw_parts_mut(bootmgfw_buffer as *mut u8, bootmgfw_size as usize) };
    bootmgfw::patch_bootmgfw(bootmgfw_buffer);

    info!("Starting bootmgfw_old.efi...");
    countdown(3);

    boot::start_image(bootmgfw_handle).convert("Cannot start image")
}

#[uefi::entry]
fn main() -> Status {
    match entrypoint() {
        Ok(()) => Status::SUCCESS,
        Err(e) => {
            error!("UEFI application failed: {}", e.data());
            countdown(5);
            e.status()
        }
    }
}
