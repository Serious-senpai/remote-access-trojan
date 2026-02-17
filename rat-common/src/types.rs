use tokio::net::ToSocketAddrs;

pub trait PortableSocketAddrs: ToSocketAddrs + Clone + Send + Sync + 'static {}

impl<T> PortableSocketAddrs for T where T: ToSocketAddrs + Clone + Send + Sync + 'static {}
