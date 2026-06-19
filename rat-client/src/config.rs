use rustls::RootCertStore;
use rustls::pki_types::ServerName;

pub struct Config {
    pub server: String,
    pub cert_server_name: ServerName<'static>,
    pub cert_trusted_roots: RootCertStore,
}
