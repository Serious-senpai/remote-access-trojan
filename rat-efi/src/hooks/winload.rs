use alloc::boxed::Box;
use alloc::vec::Vec;
use core::arch::global_asm;
use core::ffi::CStr;
use core::sync::atomic::{AtomicI64, AtomicPtr, Ordering};
use core::{mem, ptr};

use log::{error, info};
use uefi::Status;
use windows_sys::Win32::System::Diagnostics::Debug::{
    IMAGE_DIRECTORY_ENTRY_EXPORT, IMAGE_NT_HEADERS64,
};
use windows_sys::Win32::System::SystemServices::{IMAGE_DOS_HEADER, IMAGE_EXPORT_DIRECTORY};
use windows_sys::w;

use crate::hooks::types::_LOADER_PARAMETER_BLOCK;
use crate::patcher::VariablePatternFinder;
use crate::utils;

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

static ORIGINAL_OSL_FWP_KERNEL_SETUP_PHASE1_BYTES: AtomicPtr<Vec<u8>> =
    AtomicPtr::new(ptr::null_mut());

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

global_asm!(
    "OslArchTransferToKernelHooked_trampoline:",
    "movabs rax, 0",
    "call rax",
    "OslArchTransferToKernelHooked_trampoline_end:",
);

const OSL_FWP_KERNEL_SETUP_PHASE1: &[[u8; 39]] = &[[
    0xCC, // 1 byte before (int)
    0x48, 0x89, 0x4C, 0x24, 0x08, // mov [rsp - 8 + arg_0], rcx
    0x55, // push rbp
    0x53, // push rbx
    0x56, // push rsi
    0x57, // push rdi
    0x41, 0x54, // push r12
    0x41, 0x55, // push r13
    0x41, 0x56, // push r14
    0x41, 0x57, // push r15
    0x48, 0x8D, 0x6C, 0x24, 0xE1, // lea rbp, [rsp - 1Fh]
    0x48, 0x81, 0xEC, 0xB8, 0x00, 0x00, 0x00, // sub rsp, 0B8h
    0x48, 0x8B, 0xF1, // mov rsi, rcx
    0x33, 0xFF, // xor edi, edi
    0x48, 0x8D, 0x4D, 0x6F, // lea rcx, [rbp + 57h + arg_8]
]];

global_asm!(
    "OslFwpKernelSetupPhase1Hooked_trampoline:",
    "movabs rax, 0",
    "jmp rax",
    "OslFwpKernelSetupPhase1Hooked_trampoline_end:",
);

unsafe extern "efiapi" {
    fn OslArchTransferToKernelHooked_trampoline();
    fn OslArchTransferToKernelHooked_trampoline_end();
    fn OslFwpKernelSetupPhase1Hooked_trampoline();
    fn OslFwpKernelSetupPhase1Hooked_trampoline_end();
}

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
    info!("osl_arch_transfer_to_kernel_hooked called!");
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

    let cr0 = utils::DisableWriteProtection::new();
    let original = ORIGINAL_OSL_FWP_KERNEL_SETUP_PHASE1.load(Ordering::Acquire) as *mut u8;
    unsafe {
        let bytes = Box::from_raw(
            ORIGINAL_OSL_FWP_KERNEL_SETUP_PHASE1_BYTES.swap(ptr::null_mut(), Ordering::Acquire),
        );
        original.copy_from_nonoverlapping(bytes.as_ptr(), bytes.len());
    }

    unsafe {
        blp_arch_switch_context(0);
        let ntoskrnl_entry =
            utils::get_boot_loaded_module(&(*loader_block).LoadOrderListHead, w!("ntoskrnl.exe"));

        if let Some(ntoskrnl_entry) = ntoskrnl_entry {
            let ntoskrnl = ntoskrnl_entry.kldr_entry.DllBase as *mut u8;
            let dos = ntoskrnl as *mut IMAGE_DOS_HEADER;
            let nt_header =
                ntoskrnl.wrapping_byte_offset((*dos).e_lfanew as isize) as *mut IMAGE_NT_HEADERS64;

            let export_dir =
                (*nt_header).OptionalHeader.DataDirectory[IMAGE_DIRECTORY_ENTRY_EXPORT as usize];
            let exports = ntoskrnl.wrapping_byte_offset(export_dir.VirtualAddress as isize)
                as *mut IMAGE_EXPORT_DIRECTORY;

            let names =
                ntoskrnl.wrapping_byte_offset((*exports).AddressOfNames as isize) as *const u32;
            let ordinals = ntoskrnl.wrapping_byte_offset((*exports).AddressOfNameOrdinals as isize)
                as *const u16;
            let functions =
                ntoskrnl.wrapping_byte_offset((*exports).AddressOfFunctions as isize) as *const u32;

            for i in 0..(*exports).NumberOfNames {
                let name =
                    ntoskrnl.wrapping_byte_offset(*names.wrapping_offset(i as isize) as isize);
                let name = CStr::from_ptr(name as *const i8);
                if name == c"RtlRandom" || name == c"RtlRandomEx" {
                    let ordinal = *ordinals.wrapping_offset(i as isize);
                    let rva = *functions.wrapping_offset(ordinal as isize);

                    let function = ntoskrnl.wrapping_byte_offset(rva as isize);
                    let patch = &[
                        0x48, 0x89, 0x4C, 0x24, 0x08, // mov QWORD PTR [rsp+0x8], rcx
                        0x48, 0x8B, 0x44, 0x24, 0x08, // mov rax,QWORD PTR [rsp+0x8]
                        0xC7, 0x00, 0x45, 0x00, 0x00, 0x00, // mov DWORD PTR [rax], 0x45
                        0xB8, 0x45, 0x00, 0x00, 0x00, // mov eax, 0x45
                        0xC3, // ret
                    ];
                    function.copy_from_nonoverlapping(patch.as_ptr(), patch.len());
                }
            }
        }
    }

    drop(cr0);
    unsafe {
        let original_fn = mem::transmute::<*mut u8, OslFwpKernelSetupPhase1Fn>(original);
        original_fn(loader_block)
    }
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

    match VariablePatternFinder::new(OSL_ARCH_TRANSFER_TO_KERNEL).find_mut(winload) {
        Some(original) => {
            let original = &mut original[8..];
            let original_call_addr =
                original.as_ptr() as i64 + 5 + i64::from(utils::extract_call_rel32(original));
            ORIGINAL_OSL_ARCH_TRANSFER_TO_KERNEL.store(original_call_addr, Ordering::Release);

            let patched = utils::get_function_code(
                OslArchTransferToKernelHooked_trampoline,
                OslArchTransferToKernelHooked_trampoline_end,
            );
            original[..patched.len()].copy_from_slice(patched);

            let target_func_addr = osl_arch_transfer_to_kernel_hooked as *const u8 as i64;
            original[2..10].copy_from_slice(&target_func_addr.to_le_bytes());

            info!("Patched call to OslArchTransferToKernel");
        }
        None => {
            error!("Cannot patch call to OslArchTransferToKernel");
            return;
        }
    }

    match VariablePatternFinder::new(OSL_FWP_KERNEL_SETUP_PHASE1).find_mut(winload) {
        Some(original) => {
            let original = &mut original[1..];
            ORIGINAL_OSL_FWP_KERNEL_SETUP_PHASE1.store(original.as_ptr() as i64, Ordering::Release);

            let patched = utils::get_function_code(
                OslFwpKernelSetupPhase1Hooked_trampoline,
                OslFwpKernelSetupPhase1Hooked_trampoline_end,
            );
            ORIGINAL_OSL_FWP_KERNEL_SETUP_PHASE1_BYTES.store(
                Box::into_raw(Box::new(original[..patched.len()].to_vec())),
                Ordering::Release,
            );
            original[..patched.len()].copy_from_slice(patched);

            let target_func_addr = osl_fwp_kernel_setup_phase1_hooked as *const u8 as i64;
            original[2..10].copy_from_slice(&target_func_addr.to_le_bytes());

            info!("Patched OslFwpKernelSetupPhase1: {:02X?}", &original[..32]);
        }
        None => {
            error!("Cannot find OslFwpKernelSetupPhase1");
        }
    }
}
