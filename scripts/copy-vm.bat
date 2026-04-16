@echo off

for %%f in ("%~dp0.") do set root=%%~ff
cd /d %root%

mountvol S: /s
cd /d S:\EFI\Microsoft\Boot
cls
echo Quick copy template:
echo  copy %root%\rat-efi.efi S:\EFI\Microsoft\Boot\bootmgfw.efi
cmd
