@echo off

for %%f in ("%~dp0..") do set root=%%~ff
echo Found repository root: %root%

cargo install --locked --version 0.2.0 rustls-cert-gen
rustls-cert-gen --output %root%\certs --server-auth --cert-file-name cert --ca-file-name root-ca --san localhost --san 127.0.0.1
