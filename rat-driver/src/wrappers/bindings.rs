use core::arch::asm;
use core::ptr;

use wdk_sys::{
    HANDLE, OBJECT_ATTRIBUTES, PIO_STACK_LOCATION, PIRP, POBJECT_ATTRIBUTES, PSECURITY_DESCRIPTOR,
    PUNICODE_STRING, ULONG,
};

/// # Safety
/// Binding to [`IoGetCurrentIrpStackLocation`](https://codemachine.com/downloads/win71/wdm.h)
#[allow(non_snake_case)]
pub unsafe fn IoGetCurrentIrpStackLocation(irp: PIRP) -> PIO_STACK_LOCATION {
    unsafe {
        debug_assert!((*irp).CurrentLocation <= (*irp).StackCount + 1);
        (*irp)
            .Tail
            .Overlay
            .__bindgen_anon_2
            .__bindgen_anon_1
            .CurrentStackLocation
    }
}

#[allow(non_snake_case)]
pub unsafe fn InitializeObjectAttributes(
    p: POBJECT_ATTRIBUTES,
    n: PUNICODE_STRING,
    a: ULONG,
    r: HANDLE,
    s: PSECURITY_DESCRIPTOR,
) {
    unsafe {
        (*p).Length = size_of::<OBJECT_ATTRIBUTES>() as u32;
        (*p).RootDirectory = r;
        (*p).Attributes = a;
        (*p).ObjectName = n;
        (*p).SecurityDescriptor = s;
        (*p).SecurityQualityOfService = ptr::null_mut();
    }
}

pub fn debug_break() -> ! {
    unsafe {
        asm!("int3", options(noreturn));
    }
}
