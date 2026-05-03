use core::ptr;

use wdk_sys::{
    HANDLE, OBJECT_ATTRIBUTES, POBJECT_ATTRIBUTES, PSECURITY_DESCRIPTOR, PUNICODE_STRING, ULONG,
};

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
