@echo off

for %%f in ("%~dp0..") do set root=%%~ff
echo Found repository root: %root%

cargo install --locked --version 0.2.0 rustls-cert-gen
rustls-cert-gen --ecdsa-p256 --output %root%\certs --server-auth --cert-file-name cert --ca-file-name root-ca --san localhost --san 127.0.0.1 --country-name VN --organization-name "Serious-senpai/remote-access-trojan"

where openssl >nul 2>nul
if %errorlevel% equ 0 (
    openssl --version
    echo OpenSSL found. Generating client certificate...

    openssl ecparam -name prime256v1 -genkey -noout -out "%root%\certs\client.key.pem"
    echo Generated client private key

    openssl req -new -key "%root%\certs\client.key.pem" -out "%root%\certs\client.csr.pem" -subj "/CN=Operator"
    echo Generated client certificate signing request

    openssl x509 -req -in "%root%\certs\client.csr.pem" -CA "%root%\certs\root-ca.pem" -CAkey "%root%\certs\root-ca.key.pem" -CAcreateserial -out "%root%\certs\client.pem" -days 3650 -sha256
    echo Signed client certificate with root CA

    openssl pkcs12 -export -out "%root%\certs\client.pfx" -inkey "%root%\certs\client.key.pem" -in "%root%\certs\client.pem" -certfile "%root%\certs\root-ca.pem" -passout pass:
    echo Created PKCS#12 file for client certificate

    del "%root%\certs\client.key.pem"
    del "%root%\certs\client.pem"
    del "%root%\certs\client.csr.pem"

) else (
    echo OpenSSL not found. Skipping client certificate generation.
)
