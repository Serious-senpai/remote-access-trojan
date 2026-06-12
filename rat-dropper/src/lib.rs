use windows_sys::core::GUID;

pub const ESP_GUID: GUID = GUID {
    data1: 0xC12A7328,
    data2: 0xF81F,
    data3: 0x11D2,
    data4: [0xBA, 0x4B, 0x00, 0xA0, 0xC9, 0x3E, 0xC9, 0x3B],
};

include!(concat!(env!("OUT_DIR"), "/key.rs"));

pub const EFI_APPLICATION_ENCRYPTED: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/embedded.efi"));
