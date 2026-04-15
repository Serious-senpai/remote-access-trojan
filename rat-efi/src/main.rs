#![no_main]
#![no_std]

extern crate alloc;

mod error;

use alloc::string::{String, ToString};
use alloc::{format, vec};
use core::sync::atomic::{AtomicPtr, Ordering};
use core::time::Duration;
use core::{ptr, slice};

use log::{error, info, warn};
use uefi::proto::device_path::build::DevicePathBuilder;
use uefi::proto::device_path::build::media::FilePath;
use uefi::proto::loaded_image::LoadedImage;
use uefi::{CStr16, Status, boot, helpers, proto, table};
use uefi_raw::protocol::device_path::DevicePathProtocol;

use crate::error::{UefiErrorMessage, UefiResultConvertable};

type LoadImageFn = unsafe extern "efiapi" fn(
    boot_policy: uefi_raw::Boolean,
    parent_image_handle: uefi_raw::Handle,
    device_path: *const DevicePathProtocol,
    source_buffer: *const u8,
    source_size: usize,
    image_handle: *mut uefi_raw::Handle,
) -> Status;

static ORIGINAL_LOAD_IMAGE: AtomicPtr<LoadImageFn> = AtomicPtr::new(ptr::null_mut());

unsafe extern "efiapi" fn load_image_hooked(
    boot_policy: uefi_raw::Boolean,
    parent_image_handle: uefi_raw::Handle,
    device_path: *const DevicePathProtocol,
    source_buffer: *const u8,
    source_size: usize,
    image_handle: *mut uefi_raw::Handle,
) -> Status {
    match unsafe { device_path.as_ref() } {
        Some(device_path) => {
            info!("load_image_hooked: device_path = {device_path:?}");
        }
        None => {
            info!("load_image_hooked: device_path is null");
        }
    }

    countdown(5);

    let original = ORIGINAL_LOAD_IMAGE.load(Ordering::Acquire);
    match unsafe { original.as_ref() } {
        Some(original) => unsafe {
            original(
                boot_policy,
                parent_image_handle,
                device_path,
                source_buffer,
                source_size,
                image_handle,
            )
        },
        None => Status::NOT_FOUND,
    }
}

fn countdown(seconds: u64) {
    for i in 0..seconds {
        info!("Counting down {} seconds...", seconds - i);
        boot::stall(Duration::from_secs(1));
    }
}

const BOOTMGFW_PATTERNS: &[[u8; 21]] = &[
    [
        0x00, 0x48, 0x89, 0x05, 0x84, 0xD9, 0x07, 0x00, // 8 bytes before
        0xE8, 0x8F, 0xE2, 0x03, 0x00, // call Archpx64TransferTo64BitApplicationAsm
        0xE8, 0x42, 0xB9, 0xFB, 0xFF, 0x84, 0xC0, 0x74, // 8 bytes after
    ],
    [
        0x00, 0x48, 0x89, 0x05, 0xC0, 0xD9, 0x07, 0x00, // 8 bytes before
        0xE8, 0xFB, 0xE0, 0x03, 0x00, // call Archpx64TransferTo64BitApplicationAsm
        0xE8, 0x42, 0xC6, 0xFB, 0xFF, 0x84, 0xC0, 0x74, // 8 bytes after
    ],
];

fn find_bootmgfw_transfer_call(buffer: &[u8]) -> Option<usize> {
    for index in 0..buffer.len() {
        for pattern in BOOTMGFW_PATTERNS {
            if buffer[index..index + pattern.len()] == *pattern {
                return Some(index + 8); // offset of the call instruction
            }
        }
    }

    None
}

fn patch_bootmgfw(buffer: &mut [u8]) {
    match find_bootmgfw_transfer_call(buffer) {
        Some(offset) => {
            info!("Found pattern at offset {offset} in bootmgfw_old.efi, patching...");

            let buffer = &mut buffer[offset..];
            match usize::try_from(i32::from_le_bytes({
                let mut value = [0; 4];
                value.copy_from_slice(&buffer[1..5]);
                value
            })) {
                Ok(rel) => {
                    let original_call_addr = offset + 5 + rel;
                }
                Err(e) => {
                    error!(
                        "Failed to parse relative offset to Archpx64TransferTo64BitApplicationAsm: {e}"
                    );
                    return;
                }
            }
        }
        None => {
            info!("Cannot find target pattern in bootmgfw_old.efi");
        }
    }
}

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

    let mut utf16_buffer = [0; 128];
    let path_name = CStr16::from_str_with_buf(
        "\\EFI\\Microsoft\\Boot\\bootmgfw_old.efi",
        &mut utf16_buffer,
    )
    .map_err(|e| e.convert(format!("Cannot construct UCS-2 string ({e})")))?;

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

    info!("Loaded bootmgfw_old.efi. Hooking load_image...");

    match table::system_table_raw() {
        Some(mut system_table) => match unsafe { system_table.as_mut().boot_services.as_mut() } {
            Some(boot_services) => {
                ORIGINAL_LOAD_IMAGE.store(
                    boot_services.load_image as *mut LoadImageFn,
                    Ordering::Release,
                );
                boot_services.load_image = load_image_hooked;
                info!("Hooked load_image successfully.");
            }
            None => {
                warn!("Cannot get pointer to runtime services. load_image hooking will not work.");
            }
        },
        None => {
            warn!("Cannot get pointer to system table. load_image hooking will not work.");
        }
    }

    info!("Patching bootmgfw_old.efi...");
    let bootmgfw_image = boot::open_protocol_exclusive::<LoadedImage>(bootmgfw_handle)
        .convert("Cannot open protocol of bootmgfw_old.efi")?;
    let (bootmgfw_buffer, bootmgfw_size) = bootmgfw_image.info();
    let bootmgfw_buffer =
        unsafe { slice::from_raw_parts_mut(bootmgfw_buffer as *mut u8, bootmgfw_size as usize) };
    patch_bootmgfw(bootmgfw_buffer);

    info!("Starting bootmgfw_old.efi...");
    countdown(5);

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
