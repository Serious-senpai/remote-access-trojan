@echo off
setlocal enabledelayedexpansion

for %%f in ("%~dp0..") do set root=%%~ff
echo Found repository root: %root%
set current=%cd%

set profile=%~1
if "%profile%"=="" set profile=dev
echo Building with profile "%profile%"

set result=0

call :check rustc --version
call :check cargo --version

cd /d %root%
call :check cargo build -p rat-client --profile %profile%

@REM We cannot set this in .cargo/config.toml because `cargo-wdk` spawns its own process and is not affected by the config file.
@REM https://github.com/microsoft/windows-drivers-rs/blob/a90b267ccd9288d076ecbe96a7966f96f337bdc1/crates/wdk-build/src/utils.rs#L314-L328
set Version_Number=10.0.26100.0

cd /d %root%\rat-driver
call :check cargo wdk build --profile %profile%

cd /d %root%\rat-efi
call :check cargo build --profile %profile%

@REM Fun fact: Building individual workspace members results in smaller executables because the dependencies are not pulled at once.
@REM Besides, we have already built rat-client before.
cd /d %root%
call :check cargo build --profile %profile% -p rat-dropper
call :check cargo build --profile %profile% -p rat-server

cd /d %current%
exit /b %result%

:check
%*
if errorlevel 1 (
    echo ::error::Command "%*" failed with exit code !errorlevel!
    set result=1
)
exit /b 0
