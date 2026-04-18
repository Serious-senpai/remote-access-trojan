use alloc::sync::Arc;
use core::arch::{asm, naked_asm};
use core::ffi::CStr;
use core::sync::atomic::{AtomicI64, Ordering};
use core::{mem, slice};

use log::{error, info};
use uefi::Status;
use windows_sys::Win32::System::Diagnostics::Debug::{
    IMAGE_DIRECTORY_ENTRY_EXPORT, IMAGE_NT_HEADERS64,
};
use windows_sys::Win32::System::SystemServices::{IMAGE_DOS_HEADER, IMAGE_EXPORT_DIRECTORY};

use crate::hooks::types::_LOADER_PARAMETER_BLOCK;
use crate::patcher::{VariablePatternFinder, VariablePatternPatcher};
use crate::{fill_nops, utils};

type BlpArchSwitchContextFn = unsafe extern "efiapi" fn(ctx: i32) -> *mut u8;

type BlImgAllocateImageBufferFn = unsafe extern "efiapi" fn(
    image_buffer: *mut *mut u8,
    image_size: u64,
    memory_type: u32,
    preferred_attributes: u32,
    preferred_alignment: u32,
    flags: u32,
) -> Status;

type OslArchTransferToKernelFn =
    unsafe extern "efiapi" fn(loader_block: *mut _LOADER_PARAMETER_BLOCK, entrypoint: *mut u8);

type OslFwpKernelSetupPhase1Fn =
    unsafe extern "efiapi" fn(loader_block: *mut _LOADER_PARAMETER_BLOCK) -> Status;

static ORIGINAL_BLP_ARCH_SWITCH_CONTEXT: AtomicI64 = AtomicI64::new(0);
static ORIGINAL_BL_IMG_ALLOCATE_IMAGE_BUFFER: AtomicI64 = AtomicI64::new(0);
static ORIGINAL_OSL_ARCH_TRANSFER_TO_KERNEL: AtomicI64 = AtomicI64::new(0);
static ORIGINAL_OSL_FWP_KERNEL_SETUP_PHASE1: AtomicI64 = AtomicI64::new(0);

const BLP_ARCH_SWITCH_CONTEXT: &[[u8; 21]] = &[
    [
        0x8B, 0x75, 0x6F, 0xB9, 0x01, 0x00, 0x00, 0x00, // 8 bytes before
        0xE8, 0x1C, 0x6D, 0x02, 0x00, // call BlpArchSwitchContext
        0x48, 0x8B, 0x43, 0x10, 0x33, 0xC9, 0x48, 0x89, // 8 bytes after
    ],
    [
        0x8B, 0x75, 0x6F, 0xB9, 0x01, 0x00, 0x00, 0x00, // 8 bytes before
        0xE8, 0x48, 0x79, 0x02, 0x00, // call BlpArchSwitchContext
        0x48, 0x8B, 0x43, 0x10, 0x33, 0xC9, 0x48, 0x89, // 8 bytes after
    ],
];

const BL_IMG_ALLOCATE_IMAGE_BUFFER: &[[u8; 21]] = &[
    [
        0x4C, 0x89, 0x7D, 0x90, 0x89, 0x44, 0x24, 0x20, // 8 bytes before
        0xE8, 0x73, 0x05, 0x00, 0x00, // call BlImgAllocateImageBuffer
        0x48, 0x8B, 0x74, 0x24, 0x68, 0x8B, 0xD8, 0x45, // 8 bytes after
    ],
    [
        0x4C, 0x89, 0x7D, 0x98, 0x89, 0x44, 0x24, 0x20, // 8 bytes before
        0xE8, 0xA7, 0x05, 0x00, 0x00, // call BlImgAllocateImageBuffer
        0x48, 0x8B, 0x74, 0x24, 0x70, 0x8B, 0xD8, 0x45, // 8 bytes after
    ],
];

const OSL_ARCH_TRANSFER_TO_KERNEL: &[[u8; 21]] = &[
    [
        0x00, 0x49, 0x89, 0x80, 0xD0, 0x00, 0x00, 0x00, // 8 bytes before
        0xE8, 0x04, 0x28, 0x15, 0x00, // call OslArchTransferToKernel
        0x45, 0x33, 0xC0, 0x45, 0x33, 0xC9, 0x48, 0x63, // 8 bytes after
    ],
    [
        0x00, 0x49, 0x89, 0x80, 0xD0, 0x00, 0x00, 0x00, // 8 bytes before
        0xE8, 0x9C, 0x7D, 0x15, 0x00, // call OslArchTransferToKernel
        0x45, 0x33, 0xC0, 0x45, 0x33, 0xC9, 0x48, 0x63, // 8 bytes after
    ],
];

const OSL_ARCH_TRANSFER_TO_KERNEL_PATCHED: &[u8] = &[
    0x00, 0x49, 0x89, 0x80, 0xD0, 0x00, 0x00, 0x00, // 8 bytes before
    0x48, 0xB8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // mov rax, imm64
    0xFF, 0xD0, // call rax
];

const OSL_FWP_KERNEL_SETUP_PHASE1: &[[u8; 25]] = &[[
    0x81, 0xC8, 0x00, 0x00, 0x00, 0x48, 0x8B, 0xCF, // 8 bytes before
    0xE8, 0x23, 0xFD, 0xFE, 0xFF, // call OslFwpKernelSetupPhase1
    0x8B, 0xF0, // mov esi, eax
    0x85, 0xC0, // test eax, eax
    0x79, 0x0B, // jns short loc_1800165CA (needs fix later)
    0x41, 0xB8, 0x01, 0x00, 0x00, 0x00, // mov r8d, 1
]];

const OSL_FWP_KERNEL_SETUP_PHASE1_PATCHED: &[u8] = &[
    0x81, 0xC8, 0x00, 0x00, 0x00, 0x48, 0x8B, 0xCF, // 8 bytes before
    0x50, // push rax
    0x48, 0xB8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // mov rax, imm64
    0xFF, 0xD0, // call rax
    0x78, 0x0C, // js 0xC
    0x48, 0xB8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // mov rax, imm64
    0xFF, 0xE0, // jmp rax
    0x58, // pop rax
];

unsafe extern "efiapi" fn blp_arch_switch_context(ctx: i32) -> *mut u8 {
    let original = ORIGINAL_BLP_ARCH_SWITCH_CONTEXT.load(Ordering::Acquire) as *const ();
    unsafe {
        let original_fn = mem::transmute::<*const (), BlpArchSwitchContextFn>(original);
        original_fn(ctx)
    }
}

unsafe extern "efiapi" fn bl_img_allocate_image_buffer(
    image_buffer: *mut *mut u8,
    image_size: u64,
    memory_type: u32,
    preferred_attributes: u32,
    preferred_alignment: u32,
    flags: u32,
) -> Status {
    let original = ORIGINAL_BL_IMG_ALLOCATE_IMAGE_BUFFER.load(Ordering::Acquire) as *const ();
    unsafe {
        let original_fn = mem::transmute::<*const (), BlImgAllocateImageBufferFn>(original);
        original_fn(
            image_buffer,
            image_size,
            memory_type,
            preferred_attributes,
            preferred_alignment,
            flags,
        )
    }
}

unsafe extern "efiapi" fn osl_arch_transfer_to_kernel_hooked(
    loader_block: *mut _LOADER_PARAMETER_BLOCK,
    entrypoint: *mut u8,
) {
    let cr0 = utils::read_cr0();
    utils::write_cr0(cr0 & !0x10000);
    // blp_arch_switch_context(0);
    // info!("osl_arch_transfer_to_kernel_hooked called!");

    // unsafe {
    //     if let Some(ntoskrnl) = utils::find_pe_image_mut(entrypoint) {
    //         let ntoskrnl = ntoskrnl.as_mut_ptr();

    //         let dos = ntoskrnl as *mut IMAGE_DOS_HEADER;
    //         let nt_header =
    //             ntoskrnl.wrapping_byte_offset((*dos).e_lfanew as isize) as *mut IMAGE_NT_HEADERS64;

    //         let export_dir =
    //             (*nt_header).OptionalHeader.DataDirectory[IMAGE_DIRECTORY_ENTRY_EXPORT as usize];
    //         let exports = ntoskrnl.wrapping_byte_offset(export_dir.VirtualAddress as isize)
    //             as *mut IMAGE_EXPORT_DIRECTORY;

    //         let names =
    //             ntoskrnl.wrapping_byte_offset((*exports).AddressOfNames as isize) as *const u32;
    //         let ordinals = ntoskrnl.wrapping_byte_offset((*exports).AddressOfNameOrdinals as isize)
    //             as *const u16;
    //         let functions =
    //             ntoskrnl.wrapping_byte_offset((*exports).AddressOfFunctions as isize) as *const u32;

    //         for i in 0..(*exports).NumberOfNames {
    //             let name =
    //                 ntoskrnl.wrapping_byte_offset(*names.wrapping_offset(i as isize) as isize);
    //             let name = CStr::from_ptr(name as *const i8);
    //             if name == c"RtlRandom" || name == c"RtlRandomEx" {
    //                 let ordinal = *ordinals.wrapping_offset(i as isize);
    //                 let rva = *functions.wrapping_offset(ordinal as isize);

    //                 let function = ntoskrnl.wrapping_byte_offset(rva as isize);
    //                 let patch = &[
    //                     0x48, 0x89, 0x4C, 0x24, 0x08, // mov QWORD PTR [rsp+0x8], rcx
    //                     0x48, 0x8B, 0x44, 0x24, 0x08, // mov rax,QWORD PTR [rsp+0x8]
    //                     0xC7, 0x00, 0x45, 0x00, 0x00, 0x00, // mov DWORD PTR [rax], 0x45
    //                     0xB8, 0x45, 0x00, 0x00, 0x00, // mov eax, 0x45
    //                     0xC3, // ret
    //                 ];
    //                 function.copy_from_nonoverlapping(patch.as_ptr(), patch.len());
    //             }
    //         }
    //     }
    // }

    utils::write_cr0(cr0);

    let original = ORIGINAL_OSL_ARCH_TRANSFER_TO_KERNEL.load(Ordering::Acquire) as *const ();
    unsafe {
        let original_fn = mem::transmute::<*const (), OslArchTransferToKernelFn>(original);
        original_fn(loader_block, entrypoint)
    }
}

unsafe extern "efiapi" fn osl_fwp_kernel_setup_phase1_hooked(
    loader_block: *mut _LOADER_PARAMETER_BLOCK,
) -> Status {
    info!("osl_fwp_kernel_setup_phase1_hooked called!");

    let original = ORIGINAL_OSL_FWP_KERNEL_SETUP_PHASE1.load(Ordering::Acquire) as *const ();
    let status = unsafe {
        let original_fn = mem::transmute::<*const (), OslFwpKernelSetupPhase1Fn>(original);
        original_fn(loader_block)
    };

    fill_nops!(64);
    status
}

pub fn patch_winload(entrypoint: *mut u8) {
    info!("Patching winload.efi...");
    let winload = match unsafe { utils::find_pe_image_mut(entrypoint) } {
        Some(b) => b,
        None => {
            error!("Cannot find winload.efi PE image");
            return;
        }
    };

    let finder = VariablePatternFinder::new(BLP_ARCH_SWITCH_CONTEXT);
    match finder.find_ref(winload) {
        Some(modify) => {
            let modify = &modify[8..];
            let original_call_addr =
                modify.as_ptr() as i64 + 5 + i64::from(utils::extract_call_rel32(modify));
            ORIGINAL_BLP_ARCH_SWITCH_CONTEXT.store(original_call_addr, Ordering::Release);

            info!("Found BlpArchSwitchContext");
        }
        None => {
            error!("Cannot find BlpArchSwitchContext");
            return;
        }
    }

    let finder = VariablePatternFinder::new(BL_IMG_ALLOCATE_IMAGE_BUFFER);
    match finder.find_ref(winload) {
        Some(modify) => {
            let modify = &modify[8..];
            let original_call_addr =
                modify.as_ptr() as i64 + 5 + i64::from(utils::extract_call_rel32(modify));
            ORIGINAL_BL_IMG_ALLOCATE_IMAGE_BUFFER.store(original_call_addr, Ordering::Release);

            info!("Found BlImgAllocateImageBuffer");
        }
        None => {
            error!("Cannot find BlImgAllocateImageBuffer");
            return;
        }
    }

    let patcher = VariablePatternPatcher::new(
        OSL_ARCH_TRANSFER_TO_KERNEL,
        OSL_ARCH_TRANSFER_TO_KERNEL_PATCHED,
        Arc::new(|_, modify| {
            let target_func_addr = osl_arch_transfer_to_kernel_hooked as *const u8 as i64;
            modify[10..18].copy_from_slice(&target_func_addr.to_le_bytes());
        }),
    );
    match patcher.patch(winload) {
        Some(p) => {
            let original_call_addr = winload.as_ptr().wrapping_byte_add(p.offset) as i64
                + 5
                + i64::from(utils::extract_call_rel32(&p.original[8..]));
            ORIGINAL_OSL_ARCH_TRANSFER_TO_KERNEL.store(original_call_addr, Ordering::Release);

            info!("Patched call to OslArchTransferToKernel");
        }
        None => {
            error!("Cannot patch OslArchTransferToKernel");
            return;
        }
    }

    // let patcher = VariablePatternPatcher::new(
    //     OSL_FWP_KERNEL_SETUP_PHASE1,
    //     OSL_FWP_KERNEL_SETUP_PHASE1_PATCHED,
    //     Arc::new(|_, modify| {
    //         let target_func_addr = osl_fwp_kernel_setup_phase1_hooked as *const u8 as i64;
    //         modify[11..19].copy_from_slice(&target_func_addr.to_le_bytes());
    //     }),
    // );
    // match patcher.patch(winload) {
    //     Some(p) => {
    //         let original_call_addr = winload.as_ptr().wrapping_byte_add(p.offset) as i64
    //             + 5
    //             + i64::from(utils::extract_call_rel32(&p.original[8..]));
    //         ORIGINAL_OSL_FWP_KERNEL_SETUP_PHASE1.store(original_call_addr, Ordering::Release);

    //         let mut hooked = osl_fwp_kernel_setup_phase1_hooked as *mut u8;
    //         while unsafe { *hooked } != 0x90 {
    //             hooked = hooked.wrapping_byte_add(1);
    //         }

    //         let hooked = unsafe { slice::from_raw_parts_mut(hooked, p.original[13..].len()) };
    //         hooked.copy_from_slice(&p.original[13..]);

    //         info!("Patched call to OslFwpKernelSetupPhase1");
    //     }
    //     None => {
    //         error!("Cannot patch OslFwpKernelSetupPhase1");
    //     }
    // }
}
