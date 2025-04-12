use clap::Parser;
use futures::{StreamExt, TryStreamExt};
use itertools::Itertools;
use k8s_openapi::{
    api::{core::v1 as core, discovery::v1 as discovery, networking::v1 as networking},
    apimachinery::pkg::apis::meta::v1 as meta,
};
use kube::{api::Api, runtime::watcher, Client};
use log::{debug, error, info, log_enabled, trace};
use std::collections::{BTreeMap as Map, BTreeSet as Set};
use std::io::Write;
use std::net::IpAddr;
use std::sync::Arc;
use tokio::{sync::watch, time::Duration};

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[arg(short = 'n', long)]
    namespace: Option<String>,
    #[arg(long)]
    no_api: bool,
    #[arg(long, default_value = "127.0.0.1:2287")]
    api: std::net::SocketAddr,
    #[arg(long, default_value = "cluster.local")]
    cluster_domain: String,
    #[arg(long, default_value = "/run/knot/knot.sock")]
    knot_socket: String,
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::new().filter_or("RUST_LOG", "info"))
        .format_timestamp_millis()
        .init();

    let cli = Cli::parse();

    if let Some(ns) = &cli.namespace {
        info!("starting for namespace {ns}");
    } else {
        info!("starting for all namespaces");
    };

    let client: Client = kube::Config::infer().await?.try_into()?;
    let watcher_config = watcher::Config::default();
    let (mut watcher, mut cfg_rx) = KubeWatcher::new(client, watcher_config, cli.namespace);

    tokio::spawn(async move {
        if let Err(e) = watcher.run(Duration::from_secs(1)).await {
            panic!("k8s watcher failed: {e}");
        }
    });

    if !cli.no_api {
        tokio::spawn(api_server(cli.api, cfg_rx.clone()));
    }

    let mut zone_serial = 0u32;
    let mut zone = Vec::<u8>::new();

    loop {
        cfg_rx.changed().await?;
        let Some(cfg) = cfg_rx.borrow_and_update().clone() else {
            continue;
        };

        if log_enabled!(log::Level::Debug) {
            let mut buf = Vec::new();

            writeln!(buf, "proxy:")?;
            for (host, cfg) in cfg.proxy.as_ref() {
                writeln!(buf, "- {host}")?;
                if let Some(key) = &cfg.tls_secret {
                    writeln!(buf, "  - tls from {key}")?;
                }
                for (path, m) in &cfg.exact_matches {
                    writeln!(buf, "  - {path} => {}", m.iter().join(", "))?;
                }
                for (path, m) in &cfg.prefix_matches {
                    writeln!(buf, "  - {path}* => {}", m.iter().join(", "))?;
                }
                writeln!(buf, "  - * => {}", cfg.any_match.iter().join(", "))?;
            }

            writeln!(buf, "\ndns:")?;
            for (name, targets) in cfg.dns.as_ref() {
                for target in targets {
                    writeln!(buf, "  {name} {target}")?;
                }
            }

            debug!("new config received:\n{}", String::from_utf8_lossy(&buf));
        }

        {
            let ttl = 5;
            let knot_socket = cli.knot_socket.as_str();
            let cluster_zone = cli.cluster_domain.as_str();

            zone_serial += 1;
            zone.clear();

            // SOA record
            writeln!(
                zone,
                "@ 5 SOA ns.dns clusteradmin {zone_serial} 7200 1800 86400 {ttl}"
            )?;
            writeln!(zone, "@ NS localhost.localdomain.")?;

            for (name, targets) in cfg.dns.as_ref() {
                for target in targets {
                    writeln!(zone, "{name} {ttl} {target}")?;
                }
            }

            tokio::fs::write("cluster.zone", &zone).await?;

            debug!("reloading zone {cluster_zone}");
            tokio::process::Command::new("knotc")
                .args(["-s", knot_socket, "--blocking", "zone-reload", cluster_zone])
                .status()
                .await?;
        }
    }
}

#[derive(Clone, serde::Serialize)]
struct Config {
    dns: Arc<DNSConfig>,
    proxy: Arc<ProxyConfig>,
}

type ConfigReceiver = watch::Receiver<Option<Config>>;
type DNSConfig = Map<String, Vec<DNSEntry>>;
type ProxyConfig = Map<String, HostConfig>;

#[derive(Debug, serde::Serialize)]
enum DNSEntry {
    IP(IpAddr),
    Name(String),
}
impl std::fmt::Display for DNSEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        match self {
            Self::IP(ip) => match ip {
                IpAddr::V4(ip) => write!(f, "A {ip}"),
                IpAddr::V6(ip) => write!(f, "AAAA {ip}"),
            },
            Self::Name(alias) => write!(f, "CNAME {alias}"),
        }
    }
}

async fn api_server(bind: impl Into<std::net::SocketAddr>, cfg_rx: ConfigReceiver) {
    use warp::Filter;

    let server = warp::get()
        .map(move || cfg_rx.clone())
        .map(|rx: ConfigReceiver| {
            let cfg = rx.borrow().clone();
            warp::reply::json(&cfg)
        });

    warp::serve(server).try_bind(bind).await;
}

struct KubeWatcher {
    client: Client,
    watcher_config: watcher::Config,
    state: WatcherState,
    namespace: Option<String>,
    tx: watch::Sender<Option<Config>>,
}
impl KubeWatcher {
    fn new(
        client: Client,
        watcher_config: watcher::Config,
        namespace: Option<String>,
    ) -> (Self, ConfigReceiver) {
        let (tx, cfg_rx) = watch::channel(None);

        (
            Self {
                client,
                watcher_config,
                namespace,
                tx,
                state: WatcherState::new(),
            },
            cfg_rx,
        )
    }

    async fn run(&mut self, retry_delay: Duration) -> eyre::Result<()> {
        loop {
            let Err(e) = self.run_once().await else {
                break;
            };

            error!("kubernetes watcher failed (retrying): {e}");
            tokio::time::sleep(retry_delay).await;
        }
        Ok(())
    }

    async fn run_once(&mut self) -> eyre::Result<()> {
        let mut streams = match &self.namespace {
            None => WatcherStreams::all(&self.client, &self.watcher_config),
            Some(ns) => WatcherStreams::namespaced(&self.client, &self.watcher_config, ns.as_str()),
        };
        let mut was_ready = false;

        self.state.clear();

        loop {
            self.state.ingest_any_event(&mut streams).await?;

            let is_ready = self.state.is_ready();

            if is_ready && !was_ready {
                info!("all required objects received");
            } else if !is_ready && was_ready {
                info!("k8s watches restarted");
            }
            was_ready = is_ready;
            if !is_ready {
                continue;
            }

            //dump_json(&services.iter().collect::<Vec<_>>());
            //dump_json(&ep_slices.iter().collect::<Vec<_>>());
            //dump_json(&services.iter().collect::<Vec<_>>());

            // assemble DNS config
            let mut dns = DNSConfig::new();

            for (key, svc) in &self.state.services {
                let name = format!("{}.{}.svc", key.name, key.namespace);

                let targets = match &svc.target {
                    ServiceTarget::None => {
                        continue;
                    }
                    ServiceTarget::Name(name) => vec![DNSEntry::Name(name.clone())],
                    ServiceTarget::ClusterIPs(ips) => {
                        ips.iter().map(|ip| DNSEntry::IP(ip.clone())).collect()
                    }
                    ServiceTarget::Headless => {
                        EndpointSlice::for_service(key, &self.state.ep_slices)
                            .iter()
                            .map(|(_, v)| v)
                            .map(|ep_slice| ep_slice.endpoints.iter())
                            .flatten()
                            .unique()
                            .map(|(pod_name, ip)| {
                                let sub_name = format!("{pod_name}.{name}");
                                dns.entry(sub_name.clone())
                                    .or_default()
                                    .push(DNSEntry::IP(ip.clone()));
                                DNSEntry::IP(ip.clone())
                            })
                            .collect()
                    }
                };

                dns.entry(name).or_default().extend(targets);
            }

            // assemble proxy config
            let mut proxy = ProxyConfig::new();

            for (key, ing) in &self.state.ingresses {
                for rule in &ing.rules {
                    let host_config = proxy.entry(rule.host.clone()).or_default();

                    if let Some(tls_secret) = rule.tls_secret.as_ref() {
                        host_config.tls_secret = Some(ObjectKey {
                            namespace: key.namespace.clone(),
                            name: tls_secret.clone(),
                        });
                    }

                    for m in &rule.matches {
                        let mut endpoints = m.endpoints(
                            &key.namespace,
                            &self.state.services,
                            &self.state.ep_slices,
                        );

                        use PathMatch::*;
                        match &m.path_match {
                            Exact(path) => {
                                host_config.exact_matches.entry(path.clone()).or_default()
                            }
                            Prefix(path) => {
                                host_config.prefix_matches.entry(path.clone()).or_default()
                            }
                            Any => &mut host_config.any_match,
                        }
                        .append(&mut endpoints);
                    }
                }
            }

            //dump_json(&config);

            self.tx.send_replace(Some(Config {
                dns: Arc::new(dns),
                proxy: Arc::new(proxy),
            }));
        }
    }
}

type Stream<T> =
    std::pin::Pin<Box<dyn futures::Stream<Item = watcher::Result<watcher::Event<T>>> + Send>>;

struct WatcherStreams {
    svc: Stream<core::Service>,
    eps: Stream<discovery::EndpointSlice>,
    ing: Stream<networking::Ingress>,
}
impl WatcherStreams {
    fn all(client: &Client, watcher_config: &watcher::Config) -> Self {
        let svcs = Api::<core::Service>::all(client.clone());
        let svc = watcher(svcs, watcher_config.clone()).boxed();

        let epss = Api::<discovery::EndpointSlice>::all(client.clone());
        let eps = watcher(epss, watcher_config.clone()).boxed();

        let ings = Api::<networking::Ingress>::all(client.clone());
        let ing = watcher(ings, watcher_config.clone()).boxed();

        Self { svc, eps, ing }
    }

    fn namespaced(client: &Client, watcher_config: &watcher::Config, namespace: &str) -> Self {
        let svcs = Api::<core::Service>::namespaced(client.clone(), namespace);
        let svc = watcher(svcs, watcher_config.clone()).boxed();

        let epss = Api::<discovery::EndpointSlice>::namespaced(client.clone(), namespace);
        let eps = watcher(epss, watcher_config.clone()).boxed();

        let ings = Api::<networking::Ingress>::namespaced(client.clone(), namespace);
        let ing = watcher(ings, watcher_config.clone()).boxed();

        Self { svc, eps, ing }
    }
}

struct WatcherState {
    services: Map<ObjectKey, Service>,
    ep_slices: Map<EndpointSliceKey, EndpointSlice>,
    ingresses: Map<ObjectKey, Ingress>,

    svcs_ready: bool,
    epss_ready: bool,
    ings_ready: bool,
}
impl WatcherState {
    fn new() -> Self {
        Self {
            services: Map::new(),
            ep_slices: Map::new(),
            ingresses: Map::new(),
            svcs_ready: false,
            epss_ready: false,
            ings_ready: false,
        }
    }

    fn is_ready(&self) -> bool {
        self.svcs_ready && self.epss_ready && self.ings_ready
    }

    fn clear(&mut self) {
        self.svcs_ready = false;
        self.epss_ready = false;
        self.ings_ready = false;
        self.services.clear();
        self.ep_slices.clear();
        self.ingresses.clear();
    }

    async fn ingest_any_event(&mut self, streams: &mut WatcherStreams) -> eyre::Result<()> {
        tokio::select!(
          e = streams.svc.try_next() => {
              let e = e?.unwrap();
              trace!("got svc event: {e:?}");
              self.svcs_ready = ingest_event::<Service, _>(&mut self.services, e);
          },
          e = streams.eps.try_next() => {
              let e = e?.unwrap();
              trace!("got eps event: {e:?}");
              self.epss_ready = ingest_event::<EndpointSlice, _>(&mut self.ep_slices, e);
          },
          e = streams.ing.try_next() => {
              let e = e?.unwrap();
              trace!("got ing event: {e:?}");
              self.ings_ready = ingest_event::<Ingress, _>(&mut self.ingresses, e);
          },
        );
        Ok(())
    }
}

#[allow(unused)]
fn dump_json<T>(v: &T)
where
    T: serde::Serialize,
{
    use std::io::Write;
    let mut out = std::io::stdout();
    serde_json::to_writer_pretty(&out, v).unwrap();
    out.write(b"\n").unwrap();
}

#[derive(Debug, PartialEq, Eq, serde::Serialize)]
struct HostConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    tls_secret: Option<ObjectKey>,
    #[serde(skip_serializing_if = "Map::is_empty")]
    exact_matches: Map<String, Set<Endpoint>>,
    #[serde(skip_serializing_if = "Map::is_empty")]
    prefix_matches: Map<String, Set<Endpoint>>,
    #[serde(skip_serializing_if = "Set::is_empty")]
    any_match: Set<Endpoint>,
}
impl Default for HostConfig {
    fn default() -> Self {
        Self {
            tls_secret: None,
            exact_matches: Map::new(),
            prefix_matches: Map::new(),
            any_match: Set::new(),
        }
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
struct Endpoint {
    ip: IpAddr,
    port: u16,
}
impl std::fmt::Display for Endpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(f, "{}:{}", self.ip, self.port)
    }
}

fn ingest_event<T: KeyValueFrom<V>, V>(map: &mut Map<T::Key, T>, event: watcher::Event<V>) -> bool {
    use watcher::Event::*;
    match event {
        Init => {
            map.clear();
            false
        }
        InitApply(v) => {
            if let (Ok(key), Ok(value)) = (T::key_from(&v), T::value_from(&v)) {
                map.insert(key, value);
            };
            false
        }
        InitDone => true,
        Apply(v) => {
            if let (Ok(key), Ok(value)) = (T::key_from(&v), T::value_from(&v)) {
                map.insert(key, value);
            }
            true
        }
        Delete(v) => {
            if let Ok(key) = T::key_from(&v) {
                map.remove(&key);
            }
            true
        }
    }
}

fn is_tcp(protocol: &Option<String>) -> bool {
    match protocol {
        None => true,
        Some(s) => s.as_str() == "TCP",
    }
}

trait KeyValueFrom<V>: Sized {
    type Key: Ord;
    type Error;
    fn key_from(v: &V) -> Result<Self::Key, Self::Error>;
    fn value_from(v: &V) -> Result<Self, Self::Error>;
}

#[derive(serde::Serialize)]
enum ServiceTarget {
    None,
    Headless,
    ClusterIPs(Set<IpAddr>),
    Name(String),
}

#[derive(serde::Serialize)]
struct Service {
    target: ServiceTarget,
    ports: Map<u16, String>,
}
impl KeyValueFrom<core::Service> for Service {
    type Key = ObjectKey;
    type Error = &'static str;

    fn key_from(svc: &core::Service) -> Result<Self::Key, Self::Error> {
        ObjectKey::try_from(&svc.metadata)
    }

    fn value_from(svc: &core::Service) -> Result<Self, Self::Error> {
        let target = if let Some(spec) = &svc.spec {
            match spec.type_.as_ref().map(|s| s.as_str()) {
                None => ServiceTarget::None,
                Some("ExternalName") => match &spec.external_name {
                    None => ServiceTarget::None,
                    Some(name) => ServiceTarget::Name(name.clone()),
                },
                Some("ClusterIP") | Some("NodePort") | Some("LoadBalancer") => {
                    match &spec.cluster_ips {
                        None => ServiceTarget::None,
                        Some(ips) => {
                            if ips.iter().map(|s| s.as_str()).contains(&"None") {
                                ServiceTarget::Headless
                            } else {
                                ServiceTarget::ClusterIPs(
                                    ips.iter().filter_map(|ip| ip.parse().ok()).collect(),
                                )
                            }
                        }
                    }
                }
                _ => ServiceTarget::None,
            }
        } else {
            ServiceTarget::None
        };

        Ok(Self {
            target,
            ports: svc
                .spec
                .as_ref()
                .and_then(|spec| spec.ports.as_ref())
                .map(|ports| {
                    ports
                        .iter()
                        .filter(|p| is_tcp(&p.protocol))
                        .map(|p| (p.port as u16, p.name.clone().unwrap_or_default()))
                        .collect()
                })
                .unwrap_or_default(), // empty map if no ports
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
struct ObjectKey {
    namespace: String,
    name: String,
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

#[derive(serde::Serialize)]
struct EndpointSlice {
    target_ports: Map<String, u16>,
    endpoints: Set<(String, IpAddr)>,
}
impl EndpointSlice {
    fn for_service<'a>(
        service_key: &ObjectKey,
        ep_slices: &'a Map<EndpointSliceKey, Self>,
    ) -> Vec<(&'a EndpointSliceKey, &'a Self)> {
        let key_min = EndpointSliceKey {
            namespace: service_key.namespace.clone(),
            service_name: service_key.name.clone(),
            name: String::new(),
        };

        use std::ops::Bound;

        ep_slices
            .range((Bound::Included(key_min), Bound::Unbounded))
            .take_while(|(k, _)| k.is_service(service_key))
            .collect()
    }
}
impl KeyValueFrom<discovery::EndpointSlice> for EndpointSlice {
    type Key = EndpointSliceKey;
    type Error = &'static str;

    fn key_from(eps: &discovery::EndpointSlice) -> Result<Self::Key, Self::Error> {
        Ok(EndpointSliceKey {
            namespace: eps.metadata.namespace.clone().ok_or("no namespace")?,
            service_name: eps
                .metadata
                .owner_references
                .as_ref()
                .ok_or("no owners")?
                .first()
                .ok_or("empty owners")?
                .name
                .clone(),
            name: eps.metadata.name.clone().ok_or("no name")?,
        })
    }

    fn value_from(eps: &discovery::EndpointSlice) -> Result<Self, Self::Error> {
        Ok(Self {
            target_ports: eps
                .ports
                .as_ref()
                .map(|ports| {
                    ports
                        .iter()
                        .filter(|p| is_tcp(&p.protocol))
                        .filter_map(|p| {
                            let Some(port) = p.port else {
                                return None;
                            };
                            let name = p.name.clone().unwrap_or_default();
                            Some((name, port as u16))
                        })
                        .collect()
                })
                .unwrap_or_default(),

            endpoints: eps
                .endpoints
                .iter()
                .filter(|ep| {
                    // TODO topology, that requires a node context to know where we are
                    ep.conditions.as_ref().is_some_and(|c| match c.ready {
                        // None means unknown state, and should be interpreted as ready
                        // https://kubernetes.io/docs/reference/generated/kubernetes-api/v1.29/#endpointconditions-v1-discovery-k8s-io
                        Some(true) | None => true,
                        Some(false) => false,
                    })
                })
                .filter_map(|ep| {
                    let Some(pod_name) = ep.target_ref.as_ref().and_then(|t| t.name.clone()) else {
                        return None;
                    };
                    ep.addresses
                        .iter()
                        .filter_map(|addr| addr.parse().ok())
                        .next()
                        .map(|addr| (pod_name, addr))
                })
                .collect(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
struct EndpointSliceKey {
    namespace: String,
    service_name: String,
    name: String,
}
impl EndpointSliceKey {
    fn is_service(&self, key: &ObjectKey) -> bool {
        self.namespace == key.namespace && self.service_name == key.name
    }
}

#[derive(Debug, serde::Serialize)]
struct Ingress {
    rules: Vec<IngressRule>,
}
impl KeyValueFrom<networking::Ingress> for Ingress {
    type Key = ObjectKey;
    type Error = &'static str;

    fn key_from(ing: &networking::Ingress) -> Result<Self::Key, Self::Error> {
        ObjectKey::try_from(&ing.metadata)
    }

    fn value_from(ing: &networking::Ingress) -> Result<Self, Self::Error> {
        let spec = ing.spec.as_ref().ok_or("no spec")?;

        let rules = spec.rules.as_ref().map_or_else(
            || Vec::new(),
            |v| {
                v.iter()
                    .filter_map(|m| IngressRule::from_rule(m, &spec))
                    .collect()
            },
        );

        Ok(Self { rules })
    }
}

#[derive(Debug, serde::Serialize)]
struct IngressRule {
    host: String,
    tls_secret: Option<String>,
    matches: Vec<IngressMatch>,
}
impl IngressRule {
    fn from_rule(rule: &networking::IngressRule, spec: &networking::IngressSpec) -> Option<Self> {
        let Some(host) = rule.host.as_ref() else {
            return None;
        };
        Some(Self {
            host: host.clone(),
            tls_secret: spec.tls.as_ref().and_then(|tls| {
                tls.iter()
                    .find(|tls| tls.hosts.as_ref().is_some_and(|hosts| hosts.contains(host)))
                    .and_then(|tls| tls.secret_name.clone())
            }),
            matches: rule
                .http
                .as_ref()
                .map(|http| {
                    http.paths
                        .iter()
                        .filter_map(|path| IngressMatch::from_http_path(&path, spec))
                        .collect()
                })
                .unwrap_or_default(),
        })
    }
}

#[derive(Debug, serde::Serialize)]
struct IngressMatch {
    path_match: PathMatch,
    backend: Option<IngressBackend>,
}
impl IngressMatch {
    fn from_http_path(
        path_spec: &networking::HTTPIngressPath,
        spec: &networking::IngressSpec,
    ) -> Option<Self> {
        let path_match = match path_spec.path_type.as_str() {
            "Exact" => PathMatch::Exact(path_spec.path.as_ref().unwrap().clone()),
            "Prefix" | "ImplementationSpecific" => match path_spec.path.as_ref() {
                None => PathMatch::Any,
                Some(path) => match path.as_str() {
                    "" | "/" => PathMatch::Any,
                    _ => PathMatch::Prefix(path.clone()),
                },
            },
            _ => {
                return None;
            }
        };
        Some(Self {
            path_match,
            backend: IngressBackend::from_backend(&path_spec.backend).or_else(|| {
                spec.default_backend
                    .as_ref()
                    .and_then(|b| IngressBackend::from_backend(b))
            }),
        })
    }

    fn endpoints(
        &self,
        namespace: &String,
        services: &Map<ObjectKey, Service>,
        ep_slices: &Map<EndpointSliceKey, EndpointSlice>,
    ) -> Set<Endpoint> {
        let mut endpoints = Set::new();

        let Some(backend) = self.backend.as_ref() else {
            return endpoints;
        };

        let service_key = ObjectKey {
            namespace: namespace.clone(),
            name: backend.service.clone(),
        };

        let Some(service) = services.get(&service_key) else {
            return endpoints;
        };

        let port_name = match &backend.port {
            PortRef::Name(n) => Some(n),
            PortRef::Number(n) => service.ports.get(&n),
        };
        let Some(port_name) = port_name else {
            return endpoints;
        };

        for (_, slice) in EndpointSlice::for_service(&service_key, &ep_slices) {
            let Some(port) = slice.target_ports.get(port_name) else {
                continue;
            };

            for (_, addr) in &slice.endpoints {
                endpoints.insert(Endpoint {
                    ip: addr.clone(),
                    port: port.clone(),
                });
            }
        }

        endpoints
    }
}

#[derive(Debug, serde::Serialize)]
struct IngressBackend {
    service: String,
    port: PortRef,
}
impl IngressBackend {
    fn from_backend(backend: &networking::IngressBackend) -> Option<Self> {
        let Some(service) = backend.service.as_ref() else {
            return None;
        };
        let Some(port) = service.port.as_ref() else {
            return None;
        };
        let port = if let Some(number) = port.number {
            PortRef::Number(number as u16)
        } else if let Some(name) = port.name.as_ref() {
            PortRef::Name(name.clone())
        } else {
            return None;
        };
        Some(Self {
            service: service.name.clone(),
            port,
        })
    }
}

#[derive(Debug, serde::Serialize)]
enum PortRef {
    Number(u16),
    Name(String),
}

#[derive(Debug, serde::Serialize)]
enum PathMatch {
    Exact(String),
    Prefix(String),
    Any,
}
