use core::mem;

use wdk_sys::UNICODE_STRING;
use widestring::{U16CStr, Utf16Str};

pub unsafe fn to_u16cstr(s: &UNICODE_STRING) -> Option<&U16CStr> {
    if s.Buffer.is_null() {
        return None;
    }

    unsafe { U16CStr::from_ptr_truncate(s.Buffer, s.Length as usize / mem::size_of::<u16>()).ok() }
}

pub unsafe fn to_utf16str(s: &UNICODE_STRING) -> Option<&Utf16Str> {
    match unsafe { to_u16cstr(s) } {
        Some(cstr) => Utf16Str::from_ucstr(cstr).ok(),
        None => None,
    }
}
