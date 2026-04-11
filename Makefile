SHELL := /bin/bash
ROOT := $(abspath $(dir $(lastword $(MAKEFILE_LIST))))

build:
	cd /edk2 && \
	source edksetup.sh && \
	PACKAGES_PATH=/edk2:$(ROOT)/test-efi build -p $(ROOT)/test-efi/MyPkg/MyPkg.dsc -a X64 -t GCC
	mkdir -p $(ROOT)/esp/EFI/BOOT
	cp /edk2/Build/MyPkg/DEBUG_GCC/X64/TestApp.efi $(ROOT)/esp/EFI/BOOT/BOOTX64.EFI

run:
	cp /usr/share/OVMF/OVMF_VARS_4M.fd /tmp/OVMF_VARS.fd
	qemu-system-x86_64 \
		-drive if=pflash,format=raw,readonly=on,file=/usr/share/OVMF/OVMF_CODE_4M.fd \
		-drive if=pflash,format=raw,file=/tmp/OVMF_VARS.fd \
		-drive format=raw,file=fat:rw:$(ROOT)/esp
