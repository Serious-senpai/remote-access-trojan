use core::ptr;
use core::sync::atomic::{AtomicPtr, Ordering};

use log::{info, warn};
use uefi::proto::loaded_image::LoadedImage;
use uefi::{Status, boot, table};
use uefi_raw::protocol::device_path::DevicePathProtocol;

type LoadImageFn = unsafe extern "efiapi" fn(
    boot_policy: uefi_raw::Boolean,
    parent_image_handle: uefi_raw::Handle,
    device_path: *const DevicePathProtocol,
    source_buffer: *const u8,
    source_size: usize,
    image_handle: *mut uefi_raw::Handle,
) -> Status;

type ExitBootServicesFn =
    unsafe extern "efiapi" fn(image_handle: uefi_raw::Handle, map_key: usize) -> Status;

static ORIGINAL_LOAD_IMAGE: AtomicPtr<LoadImageFn> = AtomicPtr::new(ptr::null_mut());
static ORIGINAL_EXIT_BOOT_SERVICES: AtomicPtr<ExitBootServicesFn> = AtomicPtr::new(ptr::null_mut());

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

unsafe extern "efiapi" fn exit_boot_services_hooked(
    image_handle: uefi_raw::Handle,
    map_key: usize,
) -> Status {
    info!("ExitBootServices called");

    if let Some(handle) = unsafe { uefi::Handle::from_ptr(image_handle) }
        && let Ok(image) = boot::open_protocol_exclusive::<LoadedImage>(handle)
    {
        info!("image = {image:?}");
    } else {
        warn!("Unable to obtain image information");
    }

    let original = ORIGINAL_EXIT_BOOT_SERVICES.load(Ordering::Acquire);
    match unsafe { original.as_ref() } {
        Some(original) => unsafe { original(image_handle, map_key) },
        None => Status::NOT_FOUND,
    }
}

pub fn patch_system_table() {
    info!("Patching system table...");
    match table::system_table_raw() {
        Some(mut system_table) => match unsafe { system_table.as_mut().boot_services.as_mut() } {
            Some(boot_services) => {
                ORIGINAL_LOAD_IMAGE.store(
                    boot_services.load_image as *mut LoadImageFn,
                    Ordering::Release,
                );
                ORIGINAL_EXIT_BOOT_SERVICES.store(
                    boot_services.exit_boot_services as *mut ExitBootServicesFn,
                    Ordering::Release,
                );

                boot_services.load_image = load_image_hooked;
                // boot_services.exit_boot_services = exit_boot_services_hooked;
                info!("Hooked boot services successfully.");
            }
            None => {
                warn!("Cannot get pointer to boot services. Boot services hooking will not work.");
            }
        },
        None => {
            warn!("Cannot get pointer to system table. Boot services hooking will not work.");
        }
    }
}
