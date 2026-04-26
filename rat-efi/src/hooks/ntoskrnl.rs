use log::info;

use crate::patcher::return_zero_patch;
use crate::utils::pe;

pub fn patch_ntoskrnl(ntoskrnl: &mut [u8], empty_buffer: &mut [u8]) {
    info!(
        "Patching ntoskrnl.exe using a persistent buffer of {} bytes",
        empty_buffer.len()
    );

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
