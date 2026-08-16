# remote-access-trojan

[![Build](https://github.com/Serious-senpai/remote-access-trojan/actions/workflows/build.yml/badge.svg)](https://github.com/Serious-senpai/remote-access-trojan/actions/workflows/build.yml)
[![Lint](https://github.com/Serious-senpai/remote-access-trojan/actions/workflows/lint.yml/badge.svg)](https://github.com/Serious-senpai/remote-access-trojan/actions/workflows/lint.yml)

A fully-featured Remote Access Trojan (RAT) written in Rust, complete with UEFI persistence. It’s designed for both Windows and Linux environments, with a strong focus on stealth and reliable remote control.

## Capabilities

- Remote shell access for Windows (`conhost.exe`) and Linux (`/bin/bash`).
- TLS-encrypted C&C communication via `rustls` - no OpenSSL dependencies to worry about.
- Terminal frontend powered by [xterm.js](https://xtermjs.org/) in the operator control panel.
- HTTP/2 management API based on OpenAPI - straightforward to integrate with custom web frontends.

### Windows-Specific Features

- Leverages [CVE-2024-7344](https://www.welivesecurity.com/en/eset-research/under-cloak-uefi-secure-boot-introducing-cve-2024-7344/) to bypass Secure Boot.
- Survives Windows' "Reset this PC" feature (though a fresh installation from USB/DVD will reset the boot order).
- Kernel-mode self-defense that blocks memory writes, suspensions, and termination of our processes and threads.
- Doesn't trigger [PatchGuard](https://en.wikipedia.org/wiki/Kernel_Patch_Protection).
- Partially neuters Windows Defender (`WdFilter.sys` and its user-mode services).
- Requires only a single execution as Administrator to achieve persistence.

#### Demonstration

The RAT installs itself as a Windows service, granting an immediate remote shell as *NT Authority\System*:

![windows-system.png](assets/windows-system.png)

The self-defense holds up - even SYSTEM-level tools can't touch our process:

![windows-self-defense-process.png](assets/windows-self-defense-process.png)

Attempting to terminate our threads via Process Explorer? Denied:

![windows-self-defense-thread.png](assets/windows-self-defense-thread.png)

`WdFilter.sys` is the kernel-mode driver behind Windows Defender. Normally, it attaches to the minifilter stack to monitor filesystem changes (visible via `fltmc instances`):

![wdfilter-normal.png](assets/wdfilter-normal.png)

On an infected system, `sc query` still reports it as "RUNNING", but no filter instances are attached - it's effectively a dummy driver:

![wdfilter-disabled.png](assets/wdfilter-disabled.png)

Meanwhile, the user-mode Defender services are blocked from starting entirely:

![defender-services.png](assets/defender-services.png)

## Build Instructions

### Linux

Refer to the [`Dockerfile`](/Dockerfile) for the exact steps.

### Windows

Tested with Rust 1.96. You'll need the following:

- The standard `x86_64-pc-windows-msvc` target.
- The `x86_64-unknown-uefi` target.
- [`cargo-wdk`](https://crates.io/crates/cargo-wdk) - install via `cargo install cargo-wdk`. We've tested against v0.1.1 (newer versions may introduce breaking changes).
- Windows Driver Kit (WDK) build 26100.6584. Grab it from the [official download page](https://learn.microsoft.com/en-us/windows-hardware/drivers/other-wdk-downloads) or use the installer located at [`extern/wdksetup.exe`](extern/wdksetup.exe).

Once everything is in place, run [`scripts/build.bat`](/scripts/build.bat) for a debug build. For a release build, use `scripts/build.bat release`. The script is location-agnostic, so you can run it from anywhere without `cd`'ing into the repo root.

For further details, check out the [`GitHub Actions configuration`](/.github/workflows/build.yml).

## Deployment

- **Server:**
  - Linux: `rat-server`
  - Windows: `rat-server.exe`
- **Client:**
  - Linux: `rat-client`
  - Windows: `rat-dropper.exe` (run once as Administrator)

**Note:** The TLS certificate used by the server must be signed by a root certificate trusted by the client. The `rat-client` [build script](rat-client/build.rs) typically handles this automatically.

## Known Limitations

- The management API currently lacks authentication and authorization - keep it locked down.

### Windows-Specific Quirks

- UEFI persistence only works on specific Windows 10 and 11 builds.
- The C&C address is hardcoded in [`rat-driver/src/global.rs`](rat-driver/src/global.rs). There's currently no way to update it dynamically from the dropper without recompiling the embedded binary.
