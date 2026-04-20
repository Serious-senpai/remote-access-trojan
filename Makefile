SHELL := /bin/bash
ROOT := $(abspath $(dir $(lastword $(MAKEFILE_LIST))))

build:
	cd $(ROOT) && cargo build
	cd $(ROOT)/rat-efi && cargo build
	mkdir -p $(ROOT)/esp/EFI/BOOT
	cp /target/x86_64-unknown-uefi/debug/rat-efi.efi $(ROOT)/esp/EFI/BOOT/BOOTX64.efi

build-release:
	cd $(ROOT) && cargo build --release
	cd $(ROOT)/rat-efi && cargo build --release
	mkdir -p $(ROOT)/esp/EFI/BOOT
	cp /target/x86_64-unknown-uefi/release/rat-efi.efi $(ROOT)/esp/EFI/BOOT/BOOTX64.efi

clippy:
	cd $(ROOT) && cargo clippy
	cd $(ROOT)/rat-efi && cargo clippy

run-efi:
	cp /usr/share/OVMF/OVMF_VARS_4M.fd /tmp/OVMF_VARS.fd
	qemu-system-x86_64 \
		-drive if=pflash,format=raw,readonly=on,file=/usr/share/OVMF/OVMF_CODE_4M.fd \
		-drive if=pflash,format=raw,file=/tmp/OVMF_VARS.fd \
		-drive format=raw,file=fat:rw:$(ROOT)/esp
