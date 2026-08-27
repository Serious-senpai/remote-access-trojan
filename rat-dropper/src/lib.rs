pub mod cli;

include!(concat!(env!("OUT_DIR"), "/key.rs"));

pub const EFI_APPLICATION_ENCRYPTED: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/cloak64.dat"));

pub const RELOADER_EFI_ENCRYPTED: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/reloader.efi"));
