# remote-access-trojan

[![Build](https://github.com/Serious-senpai/remote-access-trojan/actions/workflows/build.yml/badge.svg)](https://github.com/Serious-senpai/remote-access-trojan/actions/workflows/build.yml)
[![Lint](https://github.com/Serious-senpai/remote-access-trojan/actions/workflows/lint.yml/badge.svg)](https://github.com/Serious-senpai/remote-access-trojan/actions/workflows/lint.yml)

A Remote Access Trojan (RAT) with UEFI persistence, fully implemented in Rust.

## Features

- Remote shell for both Windows and Linux (`conhost.exe` and `/bin/bash`).
- TCP C&C channel with TLS encryption.
    - With the help of `rustls`, OpenSSL is completely not required to work.
- Terminal frontend using [xterm.js](https://xtermjs.org/) in operator control panel.
- OpenAPI-based HTTP/2 management API, which can be easily integrated with another web frontend.
- Obfuscated dropper using XOR encryption with a 64-byte key (created by hashing the build timestamp with SHA512).
    - The [dropper](rat-dropper) is just a tool rather than a system component. You can replace it with a batch script that:
        - Verify the system is using UEFI, Secure Boot is disabled.
        - Mount ESP.
        - Copy `bootmgfw.efi` (legitimate) to `bootmgfw_old.efi`.
        - Drop `rat-efi` to `bootmgfw.efi`, `violet04.efi`.
        - Unmount ESP.

### Windows-only

- Bypass Windows reinstallation ("Reset this PC" option).
    - Cannot bypass reinstallation using a Windows installation media (USB/DVD) though. The persistence works by creating a new boot entry and setting it as default. When using an installation media, the malicious boot entry still exists, but Windows sets its legitimate boot entry as default.
- Kernel-mode self-defense: preventing our user-mode process and its threads from being memory-written, suspended or terminated.
- Does not trigger [PatchGuard](https://en.wikipedia.org/wiki/Kernel_Patch_Protection).

Because the trojan is executed as a Windows service, we automatically get a remote shell as *NT Authority\System*:

![windows-system.png](assets/windows-system.png)

Of course, the self-defense feature can prevent user-mode processes from terminating our malware, even when we are already *System*.

![windows-self-defense-process.png](assets/windows-self-defense-process.png)

Attempts to terminate our threads will also fail. For example, when trying to do so using an Administrator [*Process Explorer*](https://learn.microsoft.com/en-us/sysinternals/downloads/process-explorer):

![windows-self-defense-thread.png](assets/windows-self-defense-thread.png)

## Build instructions

### Linux
> [!IMPORTANT]  
> This section will be written in the future.

Refer to the [`Dockerfile`](/Dockerfile) for the detailed procedure.

### Windows

The build was tested with Rust 1.96. Additional required stuff beside the default target `x86_64-pc-windows-msvc` includes:
- The target `x86_64-unknown-uefi`.
- The crate [`cargo-wdk`](https://crates.io/crates/cargo-wdk) via `cargo install cargo-wdk`. The build was tested with `cargo-wdk v0.1.1`. Future versions are not guaranteed to work though.
- Windows Driver Kit (WDK) build 26100.6584. You can install from [here](https://learn.microsoft.com/en-us/windows-hardware/drivers/other-wdk-downloads), or just use the installer at [`extern/wdksetup.exe`](extern/wdksetup.exe).

After installing the above, simply run [`scripts/build.bat`](/scripts/build.bat) to build in debug mode. For release mode, run `scripts/build.bat release`. The script was designed to execute independently of the working directory, so you don't have to `cd` to the repository root or anything (and honestly, all scripts should be written this way).

Refer to the [`GitHub Actions config`](/.github/workflows/build.yml) for the detailed procedure.

## Known limitations

- No authentication nor authorization has been implemented in the management API yet.

### Windows-only

- Cannot bypass UEFI Secure Boot yet, though you can refer to [this repository](https://github.com/Wack0/CVE-2022-21894) for a proof-of-concept vulnerability.
- The C&C address is hard-coded in [`rat-driver`](rat-driver/src/global.rs). There is currently no way to dynamically set it from the dropper except modifying the embedded binary.

## Full documentation

> [!IMPORTANT]  
> Will be uploaded after I complete my graduation thesis.
