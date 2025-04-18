use eyre::Result;

pub mod resolvers;

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
    pub http2: bool,
}
