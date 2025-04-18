use eyre::Result;
use std::net::SocketAddr;

use crate::Endpoint;

pub mod cache;
pub mod dns;
pub mod kube;

/// We need an enum of provided resolvers because we cannot use Box<dyn some-async-trait>
pub enum Resolver {
    DnsHost {
        dns_suffix: Option<String>,
    },
    Kube {
        client: ::kube::Client,
        zone: Option<String>,
    },
}

impl Resolver {
    pub async fn resolve(&self, ep: &Endpoint) -> Result<Vec<SocketAddr>> {
        match self {
            Self::DnsHost { dns_suffix } => dns::host(ep, &dns_suffix).await,
            Self::Kube { client, zone } => {
                kube::Resolver::new(ep, client, zone.as_ref())
                    .resolve()
                    .await
            }
        }
    }
}
