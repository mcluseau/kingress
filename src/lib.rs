use eyre::Result;
use k8s_openapi::apimachinery::pkg::apis::meta::v1 as meta;
use std::collections::{BTreeMap as Map, HashMap};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::watch;

pub mod http1;
pub mod resolvers;

pub const ALPN_H1: &[u8] = b"\x08http/1.1";
pub const ALPN_H2: &[u8] = b"\x02h2";
pub const ALPN_H2_H1: &[u8] = b"\x02h2\x08http/1.1";

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
pub struct Endpoint {
    pub namespace: String,
    pub service: String,
    pub port: PortRef,
    pub opts: EndpointOptions,
}
impl std::fmt::Display for Endpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(f, "{}.{}:{}", self.service, self.namespace, self.port)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
pub enum PortRef {
    Number(u16),
    Name(String),
}
impl std::fmt::Display for PortRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        match self {
            Self::Number(n) => write!(f, "{n}"),
            Self::Name(n) => write!(f, "{n}"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
pub struct EndpointOptions {
    pub secure_backends: bool,
    pub ssl_redirect: bool,
    /// endpoint is HTTP/2 capable
    pub http2: bool,
    /// handle the forwarded header.
    /// Disables client<->endpoint direct copy as it wouldn't be consistent in keepalive cases.
    pub forwarded_header: bool,
    /// CORS allowed origins.
    /// Disables client<->endpoint direct copy as it wouldn't be consistent in keepalive cases.
    pub cors_allowed_origins: Option<Vec<String>>,
}

pub struct Context {
    pub hosts: HostsReceiver,
    pub resolver: resolvers::cache::Cache,
}
impl Context {
    pub fn host(&self, name: &str) -> Option<Arc<HostConfig>> {
        self.hosts.borrow().get(name).cloned()
    }

    pub async fn resolve(&self, ep: &Endpoint) -> Vec<SocketAddr> {
        self.resolver.resolve(ep).await
    }
}

pub type HostsReceiver = watch::Receiver<Arc<Hosts>>;
pub type Hosts = HashMap<String, Arc<HostConfig>>;

#[derive(Clone, serde::Serialize)]
pub struct HostConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tls_secret: Option<ObjectKey>,
    #[serde(skip_serializing_if = "Map::is_empty")]
    pub exact_matches: Map<String, Endpoint>,
    #[serde(skip_serializing_if = "Map::is_empty")]
    pub prefix_matches: Map<String, Endpoint>,
    pub any_match: Option<Endpoint>,

    #[serde(skip_serializing)]
    pub tls_key_cert: Option<Arc<CertifiedKey>>,
}

impl Default for HostConfig {
    fn default() -> Self {
        Self {
            tls_secret: None,
            exact_matches: Map::new(),
            prefix_matches: Map::new(),
            any_match: None,
            tls_key_cert: None,
        }
    }
}

impl HostConfig {
    pub fn is_any_only(&self) -> bool {
        self.exact_matches.is_empty() && self.prefix_matches.is_empty()
    }
    pub fn is_h2_ready(&self) -> bool {
        if !self.is_any_only() {
            return false;
        }
        let Some(any) = self.any_match.as_ref() else {
            return false;
        };
        any.opts.secure_backends && any.opts.http2
    }

    pub fn endpoint_for(&self, path: &str) -> Option<Endpoint> {
        if let Some(ep) = self.exact_matches.get(path) {
            Some(ep.clone())
        } else if let Some(ep) = self
            .prefix_matches
            .iter()
            .rev()
            .find_map(|(k, ep)| path.starts_with(k).then_some(ep))
        {
            Some(ep.clone())
        } else {
            self.any_match.clone()
        }
    }
}

pub struct CertifiedKey {
    pub key: openssl::pkey::PKey<openssl::pkey::Private>,
    pub cert: openssl::x509::X509,
}
impl CertifiedKey {
    pub fn from_pem(key_pem: &[u8], crt_pem: &[u8]) -> Result<Self> {
        use openssl::{pkey::PKey, x509::X509};
        Ok(Self {
            key: PKey::private_key_from_pem(key_pem)?,
            cert: X509::from_pem(crt_pem)?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
pub struct ObjectKey {
    pub namespace: String,
    pub name: String,
}
impl TryFrom<&meta::ObjectMeta> for ObjectKey {
    type Error = &'static str;
    fn try_from(metadata: &meta::ObjectMeta) -> Result<Self, Self::Error> {
        Ok(Self {
            namespace: metadata.namespace.clone().ok_or("no namespace")?,
            name: metadata.name.clone().ok_or("no name")?,
        })
    }
}
impl std::fmt::Display for ObjectKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(f, "{}/{}", self.namespace, self.name)
    }
}
