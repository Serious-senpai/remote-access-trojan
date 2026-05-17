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

- Kernel-mode self-defense: prevent registry modification, process termination.
- Survive OS reinstall (but not hard-disk wipe unfortunately).

Because the trojan is executed as a Windows service, we automatically get a remote shell as *NT Authority\System*:

![windows-system.png](assets/windows-system.png)

Of course, the self-defense feature can prevent user-mode processes from terminating our malware, even when we are already *NT Authority\System*.

![windows-self-defense.png](assets/windows-self-defense.png)

## Known limitations

### Windows-only

- The object callback registered by [`ObRegisterCallbacks`](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/wdm/nf-wdm-obregistercallbacks) is causing [KPP](https://en.wikipedia.org/wiki/Kernel_Patch_Protection) to crash down the system.
    - TODO: Implement `KeBugCheckEx` hook to restore the crashing thread to the initial state, as described [here](http://uninformed.org/index.cgi?v=3&a=3&p=17).
