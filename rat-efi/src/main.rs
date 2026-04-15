#![no_main]
#![no_std]

extern crate alloc;

mod error;

use alloc::string::{String, ToString};
use alloc::{format, vec};
use core::time::Duration;

use log::{error, info};
use uefi::proto::device_path::build::DevicePathBuilder;
use uefi::proto::device_path::build::media::FilePath;
use uefi::proto::loaded_image::LoadedImage;
use uefi::{CStr16, Status, boot, helpers, proto};

use crate::error::UefiErrorMessage;

fn countdown(seconds: u64) {
    for i in 0..seconds {
        info!("Counting down {} seconds...", seconds - i);
        boot::stall(Duration::from_secs(1));
    }
}

fn entrypoint() -> uefi::Result<(), String> {
    helpers::init().map_err(|e| e.convert("Cannot initialize helpers"))?;
    info!("Loading bootmgfw_old.efi...");

    let handle = boot::image_handle();
    let loaded_image_protocol = boot::open_protocol_exclusive::<LoadedImage>(handle)
        .map_err(|e| e.convert("Cannot open protocol of loaded image"))?;
    let device_path = loaded_image_protocol
        .device()
        .ok_or_else(|| uefi::Error::new(Status::NOT_FOUND, "Cannot get device handle".to_string()))?
        .device_path()
        .map_err(|e| e.convert("Cannot get device path protocol".to_string()))?;

    let mut utf16_buffer = [0; 128];
    let path_name = CStr16::from_str_with_buf(
        "\\EFI\\Microsoft\\Boot\\bootmgfw_old.efi",
        &mut utf16_buffer,
    )
    .map_err(|e| e.convert(format!("Cannot construct UCS-2 string ({})", e)))?;

    let mut buffer = vec![];
    let device_path = device_path
        .append_path(
            DevicePathBuilder::with_vec(&mut buffer)
                .push(&FilePath { path_name })
                .map_err(|e| e.convert(format!("Cannot construct device path: {}", e)))?
                .finalize()
                .map_err(|e| e.convert(format!("Cannot construct device path: {}", e)))?,
        )
        .map_err(|e| e.convert("Cannot append to device path"))?;

    let image = boot::load_image(
        handle,
        boot::LoadImageSource::FromDevicePath {
            device_path: &device_path,
            boot_policy: proto::BootPolicy::ExactMatch,
        },
    )
    .map_err(|e| e.convert("Cannot load image"))?;

    info!("Loaded bootmgfw_old.efi, starting...");
    countdown(5);

    boot::start_image(image).map_err(|e| e.convert("Cannot start image"))
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
