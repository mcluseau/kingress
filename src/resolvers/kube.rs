use crate::{Endpoint, PortRef};
use eyre::{format_err, Result};
use k8s_openapi::api::{core::v1 as core, discovery::v1::EndpointSlice};
use kube::{
    api::{Api, ListParams},
    Client,
};
use std::net::{IpAddr, SocketAddr};

pub struct Resolver<'t> {
    ep: &'t Endpoint,
    client: &'t Client,
    zone: Option<&'t String>,
}
impl<'t> Resolver<'t> {
    pub fn new(ep: &'t Endpoint, client: &'t Client, zone: Option<&'t String>) -> Self {
        Self { ep, client, zone }
    }

    fn api<K>(&self) -> Api<K>
    where
        K: kube::api::Resource<Scope = k8s_openapi::NamespaceResourceScope>,
        <K as kube::Resource>::DynamicType: Default,
    {
        Api::namespaced(self.client.clone(), &self.ep.namespace)
    }

    pub async fn resolve(self) -> Result<Vec<SocketAddr>> {
        if self.zone.is_some() {
            // a zone is provided, we must filter from endpointslices ourselves
            return self.resolve_using_endpoint_slices(None).await;
        }

        let svc = self.get_service().await?;

        let Some(ref cluster_ip) = svc.cluster_ip else {
            return Err(format_err!("service has no cluster IP"));
        };

        if cluster_ip == "None" {
            // headless service, we must use endpointslices
            return self.resolve_using_endpoint_slices(Some(svc)).await;
        }

        // the simple service case, relying on kube-proxy

        let port = match self.ep.port {
            PortRef::Number(n) => n,
            PortRef::Name(ref n) => (svc.ports.iter().flatten())
                .find(|p| p.name.as_ref() == Some(n))
                .map(|p| p.port as u16)
                .ok_or_else(|| format_err!("no port named {n}"))?,
        };

        Ok((svc.cluster_ips.iter().flatten())
            .filter_map(|ip| ip.parse::<IpAddr>().ok())
            .map(|ip| SocketAddr::new(ip, port))
            .collect())
    }

    async fn get_service(&self) -> Result<core::ServiceSpec> {
        let svcs = self.api::<core::Service>();
        let svc = (svcs.get(&self.ep.service).await)
            .map_err(|e| format_err!("failed to get service: {e}"))?;

        let Some(spec) = svc.spec else {
            return Err(format_err!("service has no spec"));
        };

        Ok(spec)
    }

    async fn resolve_using_endpoint_slices(
        &self,
        svc: Option<core::ServiceSpec>,
    ) -> Result<Vec<SocketAddr>> {
        let port_name = Some(match self.ep.port {
            PortRef::Name(ref n) => n.clone(),
            PortRef::Number(port) => {
                // need the service to resolve the number to a name (why endpointslices don't
                // keep this information, with a model matching service ports? No idea...)
                //
                // Reminder: port name is required to match the target port of endpoints. We
                // can't just use the port number as-is.
                //
                let svc = match svc {
                    Some(svc) => svc,
                    None => self.get_service().await?,
                };
                let name = (svc.ports.into_iter().flatten())
                    .filter(|p| p.port == port as i32)
                    .find_map(|p| p.name)
                    .ok_or_else(|| format_err!("no port with number {port}"))?;
                name
            }
        });

        let api = self.api::<EndpointSlice>();
        let labels = &format!("kubernetes.io/service-name={}", &self.ep.service);
        let ep_slices = api.list(&ListParams::default().labels(labels)).await?;

        return Ok((ep_slices.items.into_iter())
            .filter_map(|slice| {
                let port = (slice.ports?.into_iter())
                    .filter(|p| p.name == port_name)
                    .find_map(|p| p.port)? as u16;

                let iter = (slice.endpoints.into_iter())
                    .filter(|ep| self.zone.is_none_or(|z| Some(z) == ep.zone.as_ref()))
                    .map(|ep| ep.addresses.into_iter())
                    .flatten()
                    .filter_map(|addr| addr.parse::<IpAddr>().ok())
                    .map(move |ip| SocketAddr::new(ip, port));
                Some(iter)
            })
            .flatten()
            .collect());
    }
}
