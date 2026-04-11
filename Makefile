SHELL := /bin/bash

setup:
	cd extern/edk2 && \
	$(MAKE) -C BaseTools clean && \
	$(MAKE) -C BaseTools

build:
	cd extern/edk2 && \
	source edksetup.sh && \
	PACKAGES_PATH=$$(pwd):$$(pwd)/../../test-efi build -p ../../test-efi/MyPkg/MyPkg.dsc -a X64 -t GCC
	rm -rf esp/
	mkdir -p esp/EFI/Microsoft/Boot
	cp extern/edk2/Build/MyPkg/DEBUG_GCC/X64/TestApp.efi esp/EFI/Microsoft/Boot/bootmgfw.efi

run:
	$(MAKE) copy_ovmf
	qemu-system-x86_64 \
		-drive if=pflash,format=raw,readonly=on,file=/usr/share/OVMF/OVMF_CODE_4M.fd \
		-drive if=pflash,format=raw,file=/tmp/OVMF_VARS.fd \
		-drive format=raw,file=fat:rw:esp

copy_ovmf:
	cp /usr/share/OVMF/OVMF_VARS_4M.fd /tmp/OVMF_VARS.fd
