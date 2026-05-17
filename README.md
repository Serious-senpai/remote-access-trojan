# remote-access-trojan

[![Build](https://github.com/Serious-senpai/remote-access-trojan/actions/workflows/build.yml/badge.svg)](https://github.com/Serious-senpai/remote-access-trojan/actions/workflows/build.yml)
[![Lint](https://github.com/Serious-senpai/remote-access-trojan/actions/workflows/lint.yml/badge.svg)](https://github.com/Serious-senpai/remote-access-trojan/actions/workflows/lint.yml)

A Remote Access Trojan (RAT) with UEFI persistence.

## Features

- Reverse TCP shell for both Windows and Linux.
- OpenAPI-based management API, easily integrated with another web frontend.
- Kernel-mode self-defense (Windows only).
- Survive OS reinstall (but not hard-disk wipe, Windows only).

Because the trojan is executed as a Windows service, we automatically got a remote shell as *NT Authority\System*:

![windows-system.png](assets/windows-system.png)
