@echo off

for %%f in ("%~dp0..") do set root=%%~ff
cd /d %root%

echo Found repository root: %root%

set profile=%~1
if "%profile%"=="" set profile=release
echo Building with profile "%profile%"

cd /d %root%
cargo build -p rat-client --profile %profile%

cd /d %root%\rat-driver
cargo wdk build --profile %profile%

cd /d %root%\rat-efi
cargo build --profile %profile%

cd /d %root%
cargo build --profile %profile%
