use core::ptr;
use core::sync::atomic::{AtomicPtr, Ordering};

use log::{info, warn};
use uefi::{Status, table};
use uefi_raw::protocol::device_path::DevicePathProtocol;

use crate::utils::countdown;

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

pub fn patch_load_image() {
    info!("Patching load_image...");
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
}
