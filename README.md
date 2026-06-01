# remote-access-trojan

[![Build](https://github.com/Serious-senpai/remote-access-trojan/actions/workflows/build.yml/badge.svg)](https://github.com/Serious-senpai/remote-access-trojan/actions/workflows/build.yml)
[![Lint](https://github.com/Serious-senpai/remote-access-trojan/actions/workflows/lint.yml/badge.svg)](https://github.com/Serious-senpai/remote-access-trojan/actions/workflows/lint.yml)

A Remote Access Trojan (RAT) with UEFI persistence, fully implemented in Rust.

## Features

- Reverse TCP shell for both Windows and Linux.
- Terminal frontend using [xterm.js](https://xtermjs.org/) in operator control panel.
- OpenAPI-based HTTP/2 management API, which can be easily integrated with another web frontend.
    - With the help of `rustls`, OpenSSL is completely not required for the malware to work.

### Windows-only

- Kernel-mode self-defense: preventing process termination.
- Survive OS reinstall (but not hard-disk wipe unfortunately, who would delete their ESP anyway?).
- Does not trigger [PatchGuard](https://en.wikipedia.org/wiki/Kernel_Patch_Protection).

Because the trojan is executed as a Windows service, we automatically get a remote shell as *NT Authority\System*:

![windows-system.png](assets/windows-system.png)

Of course, the self-defense feature can prevent user-mode processes from terminating our malware, even when we are already *System*.

![windows-self-defense.png](assets/windows-self-defense.png)

## Build instructions

### Linux
> [!IMPORTANT]  
> This section will be written in the future.

Refer to the [`Dockerfile`](/Dockerfile) for the detailed procedure.

### Windows

The build was tested with Rust 1.96. Additional required stuff beside the default target `x86_64-pc-windows-msvc` includes:
- The target `x86_64-unknown-uefi`.
- The crate [`cargo-wdk`](https://crates.io/crates/cargo-wdk) via `cargo install cargo-wdk`. The build was tested with `cargo-wdk v0.1.1`. Future versions are not guaranteed to work though.

Not sure if [Windows Driver Kit (WDK)](https://learn.microsoft.com/en-us/windows-hardware/drivers/download-the-wdk) is required or not. This needs to be confirmed in the future.

After installing the above, simply run [`scripts/build.bat`](/scripts/build.bat) to build in debug mode. For release mode, run `scripts/build.bat release`. The script was designed to execute independently of the working directory, so you don't have to `cd` to the repository root or anything (and honestly, all scripts should be written this way).

Refer to the [`GitHub Actions config`](/.github/workflows/build.yml) for the detailed procedure.

## Known limitations

### Windows-only

- Cannot bypass UEFI Secure Boot yet.
