use eyre::Result;
use std::net::SocketAddr;
use tokio::net;

use crate::Endpoint;

pub async fn host(ep: &Endpoint, dns_suffix: &Option<String>) -> Result<Vec<SocketAddr>> {
    let port = &ep.port;

    let full_host = endpoint_dn(ep, &dns_suffix);
    let full_host = format!("{full_host}:{port}");

    Ok(net::lookup_host(full_host).await?.collect())
}

fn endpoint_dn(ep: &Endpoint, suffix: &Option<String>) -> String {
    let service = &ep.service;
    let namespace = &ep.namespace;

    match suffix {
        None => format!("{service}.{namespace}.svc"),
        Some(suffix) => format!("{service}.{namespace}.svc.{suffix}."),
    }
}
