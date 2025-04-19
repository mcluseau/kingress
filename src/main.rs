use clap::Parser;
use eyre::{format_err, Result};
use futures::{StreamExt, TryStreamExt};
use k8s_openapi::api::{core::v1 as core, networking::v1 as networking};
use kube::{api::Api, runtime::watcher, Client};
use log::{debug, error, info, trace, warn};
use openssl::ssl;
use std::{
    collections::BTreeMap as Map,
    net::SocketAddr,
    pin::Pin,
    sync::{Arc, OnceLock},
    time::Duration,
};
use tokio::{
    io::{self, AsyncRead, AsyncWrite, AsyncWriteExt},
    net,
    sync::watch,
};

use kingress::*;

const LEGACY_XFORWARDED: bool = true;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    /// Kubernetes namespace to watch. All namespaces are watched if not set.
    #[arg(short = 'n', long)]
    namespace: Option<String>,

    /// Disable the kingress API to check internal state.
    #[arg(long)]
    no_api: bool,
    /// API server bind address
    #[arg(long, default_value = "[::1]:2287")]
    api: SocketAddr,

    /// HTTP server bind address
    #[arg(long, default_value = "[::]:80")]
    http: SocketAddr,
    /// HTTPS server bind address
    #[arg(long, default_value = "[::]:443")]
    https: SocketAddr,

    /// Method to resolve service endpoints
    #[arg(long, default_value = "kube")]
    resolver: Resolver,
    /// Size of the resolver cache. 0 disables caching.
    #[arg(long, default_value = "256")]
    resolver_cache_size: usize,
    /// Resolutions expiration delay in seconds.
    #[arg(long, default_value = "5")]
    resolver_cache_expiry: u64,
    /// Failed resolutions expiration delay in seconds.
    #[arg(long, default_value = "1")]
    resolver_cache_negative_expiry: u64,

    /// DNS suffix used by the dns-host resolver to form service FQDNs. If not set, rely on resolv.conf.
    #[arg(long)]
    cluster_domain: Option<String>,

    /// Zone used by the kube resolver to filter endpoints, if set.
    #[arg(long)]
    kube_zone: Option<String>,
}

#[derive(Clone, clap::ValueEnum)]
enum Resolver {
    /// make DNS A queries
    DnsHost,
    /// ask kube-apiserver
    Kube,
}
impl std::fmt::Display for Resolver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        let v = clap::ValueEnum::to_possible_value(self);
        f.write_str(v.as_ref().map_or("<?>", |p| p.get_name()))
    }
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
    let (mut watcher, hosts_rx) = KubeWatcher::new(client.clone(), cli.namespace);

    info!("using endpoint resolver {}", cli.resolver);

    let resolver = resolvers::cache::Builder {
        size: cli.resolver_cache_size,
        expiry_secs: cli.resolver_cache_expiry,
        negative_expiry_secs: cli.resolver_cache_negative_expiry,
        resolver: match cli.resolver {
            Resolver::DnsHost => resolvers::Resolver::DnsHost {
                dns_suffix: cli.cluster_domain,
            },
            Resolver::Kube => resolvers::Resolver::Kube {
                client: client.clone(),
                zone: cli.kube_zone,
            },
        },
    }
    .build();

    let ctx = Context {
        hosts: hosts_rx,
        resolver,
    };
    if !CTX.set(ctx).is_ok() {
        panic!("config already set");
    }

    let mut join = tokio::task::JoinSet::new();

    join.spawn(async move {
        if let Err(e) = watcher.run(Duration::from_secs(1)).await {
            panic!("k8s watcher failed: {e}");
        }
    });

    if !cli.no_api {
        info!("starting API on {}", cli.api);
        join.spawn(api_server(cli.api));
    }

    join.spawn(http_server(cli.http));
    join.spawn(https_server(cli.https));

    if let Err(e) = join.join_next().await.unwrap() {
        error!("a process failed: {e}");
    } else {
        error!("a process stopped with no error");
    }
    std::process::exit(1);
}

static CTX: OnceLock<Context> = OnceLock::new();
fn ctx() -> &'static Context {
    CTX.get().expect("config accessed before initialization")
}

async fn http_server(bind: SocketAddr) {
    info!("starting HTTP on {bind}");

    let listener = (net::TcpListener::bind(bind).await).expect("HTTP failed to listen");

    loop {
        let (sock, remote) = listener.accept().await.expect("HTTP listener failed");

        tokio::spawn(handle_http1_connection(sock, remote, "http"));
    }
}

async fn https_server(bind: SocketAddr) {
    info!("starting HTTPS on {bind}");

    let listener = (net::TcpListener::bind(bind).await).expect("HTTPS failed to listen");

    let ssl_ctx = build_server_ssl_context();

    loop {
        let (sock, remote) = listener.accept().await.expect("HTTPS listener failed");

        let ssl = ssl::Ssl::new(&ssl_ctx.clone())
            .inspect_err(|e| error!("failed to setup SSL: {e}"))
            .expect("SSL setup shouldn't fail");

        tokio::spawn(handle_https_connection(sock, remote, ssl));
    }
}

async fn handle_https_connection(sock: net::TcpStream, remote: SocketAddr, ssl: ssl::Ssl) {
    let Ok(mut stream) = tokio_openssl::SslStream::new(ssl, sock)
        .inspect_err(|e| info!("failed to create SSL stream: {e}"))
    else {
        return;
    };

    if let Err(e) = Pin::new(&mut stream).accept().await {
        debug!("{remote}: TLS not accepted: {e}");
        return;
    }

    match stream.ssl().selected_alpn_protocol() {
        // HTTP/2
        Some(b"h2") => handle_http2_connection(stream, remote).await,
        // HTTP/1.x by default
        _ => handle_http1_connection(stream, remote, "https").await,
    }
}

fn build_server_ssl_context() -> ssl::SslContext {
    use ssl::{AlpnError, NameType, SniError, SslContextBuilder, SslMethod};

    let mut builder = SslContextBuilder::new(SslMethod::tls_server()).unwrap();
    builder.set_servername_callback(move |ssl, _alert| {
        let Some(server_name) = ssl.servername(NameType::HOST_NAME) else {
            debug!("no server name provided");
            return Err(SniError::ALERT_FATAL);
        };

        let Some(host_cfg) = ctx().host(server_name) else {
            debug!("unknown host: {server_name}");
            return Err(SniError::ALERT_FATAL);
        };
        let Some(key_cert) = host_cfg.tls_key_cert.as_ref() else {
            debug!("host {server_name} has no certificate");
            return Err(SniError::ALERT_FATAL);
        };

        ssl.set_private_key(&key_cert.key)
            .map_err(|_| SniError::ALERT_FATAL)?;
        ssl.set_certificate(&key_cert.cert)
            .map_err(|_| SniError::ALERT_FATAL)?;

        Ok(())
    });

    builder.set_alpn_select_callback(move |ssl, client_protos| {
        let Some(server_name) = ssl.servername(NameType::HOST_NAME) else {
            return Err(AlpnError::ALERT_FATAL);
        };
        let Some(host_cfg) = ctx().host(server_name) else {
            return Err(AlpnError::ALERT_FATAL);
        };

        let server_protos = if host_cfg.is_h2_ready() {
            b"\x02h2\x08http/1.1".as_slice()
        } else {
            b"\x08http/1.1".as_slice()
        };

        ssl::select_next_proto(server_protos, client_protos).ok_or(AlpnError::ALERT_FATAL)
    });

    builder.build()
}

fn http_response(status: &str, message: &str) -> Vec<u8> {
    format!(
        "HTTP/1.1 {}\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{}\r\n",
        status,
        message.len() + 2,
        message
    )
    .into_bytes()
}

macro_rules! http_response {
    ($status:expr) => {
        &http_response($status, $status)
    };
}

mod http1;

async fn handle_http1_connection<RW>(mut sock: RW, remote: SocketAddr, forwarded_proto: &str)
where
    RW: AsyncRead + AsyncWrite + Unpin,
{
    macro_rules! reply {
        ($status:expr) => {{
            let _ = sock.write(&http_response($status, $status)).await;
            return;
        }};
        ($status:expr, $message:expr) => {{
            let _ = sock.write(&http_response($status, $message)).await;
            return;
        }};
    }
    macro_rules! reply_bad_request {
        ($message:expr) => {
            reply!("400 Bad Request", $message)
        };
    }

    use http1::{Error, HeaderRead};

    macro_rules! http1_result {
        ($e:expr, $limit_error:expr) => {
            match $e.await {
                Ok(e) => e,
                Err(Error::LimitReached) => reply!($limit_error),
                Err(Error::InvalidInput) => reply_bad_request!("invalid input"),
                Err(e) => {
                    debug!("{remote}: {}", e);
                    return;
                }
            }
        };
    }

    let mut read = tokio::io::BufReader::with_capacity(4096, &mut sock);
    let mut reader = http1::Reader::new(&mut read, 16 << 10);

    let req_line = http1_result!(reader.request_line(8192), "414 URI Too Long");

    let Ok(full_req_path) = std::str::from_utf8(req_line.path()) else {
        reply_bad_request!("invalid path");
    };
    let req_path = (full_req_path.find('?')).map_or(full_req_path, |i| &full_req_path[..i]);

    let header = http1_result!(reader.header(512), "413 Content Too Large");
    let header = match header {
        HeaderRead::Header(hdr) => hdr,
        HeaderRead::EndOfHeader => reply_bad_request!("no Host header"),
    };

    if !header.name().eq_ignore_ascii_case(b"host") {
        reply_bad_request!("first header must be Host");
    }

    let Ok(host) = std::str::from_utf8(header.value()) else {
        reply_bad_request!("invalid host");
    };

    let host = host.trim_ascii().to_lowercase();
    let host = host.as_str();
    let host = host.find(':').map_or(host, |i| &host[..i]);

    debug!("{remote}: requested {host} {req_path}");

    let Some(host_cfg) = ctx().host(host) else {
        reply!("404 Not Found", "Unknown host");
    };

    let Some(endpoint) = host_cfg.endpoint_for(req_path) else {
        reply!("503 Service Unavailable");
    };

    debug!("{remote}: mapped to {endpoint}");

    if endpoint.opts.ssl_redirect && forwarded_proto != "https" {
        let target_url = format!("https://{host}{full_req_path}");
        let _ = sock
            .write(
                format!("HTTP/1.1 301 Moved Permanently\r\nLocation: {target_url}\r\n\r\n<a href=\"{target_url}\">Moved Permanently</a>.\n")
                    .as_bytes(),
            )
            .await;
        return;
    }

    let (backend, backend_addr) = match connect_to_backend(&endpoint).await {
        Ok(b) => b,
        Err(BackendError::LookupFailed) => reply!("503 Service Unavailable"),
        Err(BackendError::ConnectFailed) => reply!("502 Bad Gateway"),
    };

    let mut initial_data = http1::Writer::new();
    initial_data.append(req_line.into_raw());
    initial_data.append(header.into_raw());

    initial_data.header(
        "Forwarded",
        &format!("for=\"{remote}\";proto={forwarded_proto};host={host}"),
    );
    if LEGACY_XFORWARDED {
        initial_data.header("X-Forwarded-For", &remote.to_string());
        initial_data.header("X-Forwarded-Proto", forwarded_proto);
        initial_data.header("X-Forwarded-Host", host);
    }

    let can_keepalive = host_cfg.is_any_only();
    let mut sent_connection_header = false;

    loop {
        let header = http1_result!(reader.header(4096), "413 Content Too Large");

        use http1::HeaderRead::*;
        match header {
            Header(hdr) => {
                if hdr.is(b"forwarded")
                    || hdr.is(b"x-forwarded-for")
                    || hdr.is(b"x-forwarded-proto")
                    || hdr.is(b"x-forwarded-host")
                {
                    continue;
                }

                let value = hdr.value().trim_ascii();
                if hdr.is(b"connection") {
                    sent_connection_header = true;
                    if b"keep-alive".eq_ignore_ascii_case(value) && !can_keepalive {
                        // we can't guarantee that subsequent requests are going to the same endpoint
                        initial_data.header("Connection", "close");
                        continue;
                    }
                }

                initial_data.append(hdr.into_raw());
            }
            EndOfHeader => {
                break;
            }
        };
    }

    if !sent_connection_header && !can_keepalive {
        initial_data.header("Connection", "close");
    }
    // TODO enforce "Connection: close" in the serveur response. For now we rely on endpoints
    // respecting client's choice.

    // finalize the header
    initial_data.append_slice(b"\r\n");

    // don't forget the data in the read buffer
    initial_data.append_slice(read.buffer());

    let initial_data = initial_data.into_bytes();

    let result = if endpoint.opts.secure_backends {
        let backend = match connect_tls(backend, &endpoint, b"\x08http/1.1").await {
            Ok(b) => b,
            Err(e) => {
                warn!("{backend_addr}: tls failed: {e}");
                return;
            }
        };

        forward_to_backend(initial_data, read.into_inner(), backend).await
    } else {
        forward_to_backend(initial_data, read.into_inner(), backend).await
    };

    if let Err(e) = result {
        warn!("{remote}: forwarding to {backend_addr} failed: {e}");
    }
}

async fn handle_http2_connection(
    stream: tokio_openssl::SslStream<net::TcpStream>,
    remote: SocketAddr,
) {
    // HTTP/2 conditions are met: ingress with a single any match
    // This allows direct copy of the client/backend stream.

    // SNI is required -> servername is always set
    let server_name =
        (stream.ssl().servername(ssl::NameType::HOST_NAME)).expect("servername should be set");

    let Some(host_cfg) = ctx().host(server_name) else {
        error!("{remote}: host {server_name} vanished");
        return;
    };

    let Some(ref endpoint) = host_cfg.any_match else {
        error!("{remote}: host {server_name} lost its \"*\" match");
        return;
    };

    let Ok((backend, backend_addr)) = connect_to_backend(endpoint).await else {
        return;
    };
    let Ok(backend) = connect_tls(backend, endpoint, b"\x02h2").await else {
        return;
    };

    if let Err(e) = forward_to_backend(Vec::new(), stream, backend).await {
        warn!("{remote}: forwarding to {backend_addr} failed: {e}");
    }
}

#[derive(Debug)]
enum BackendError {
    LookupFailed,
    ConnectFailed,
}

async fn connect_to_backend(
    endpoint: &Endpoint,
) -> std::result::Result<(net::TcpStream, SocketAddr), BackendError> {
    let mut backends = ctx().resolve(endpoint).await;

    if backends.is_empty() {
        return Err(BackendError::LookupFailed);
    }

    let backend_addr = match backends.len() {
        0 => return Err(BackendError::LookupFailed),
        1 => backends[0],
        n => backends[fastrand::usize(..n)],
    };

    fastrand::shuffle(&mut backends);

    for backend in backends {
        let Ok(stream) = (net::TcpStream::connect(backend).await)
            .inspect_err(|e| warn!("{endpoint}: failed to connect to {backend_addr}: {e}"))
        else {
            continue;
        };
        return Ok((stream, backend_addr));
    }

    Err(BackendError::ConnectFailed)
}

async fn connect_tls(
    stream: net::TcpStream,
    _endpoint: &Endpoint,
    alpn_protos: &[u8],
) -> Result<tokio_openssl::SslStream<net::TcpStream>> {
    use ssl::{Ssl, SslContextBuilder, SslMethod, SslVerifyMode};

    let mut ssl_ctx = SslContextBuilder::new(SslMethod::tls_client())?;

    ssl_ctx.set_alpn_protos(alpn_protos)?;

    // TODO add server-name annotation and check it if set
    ssl_ctx.set_verify(SslVerifyMode::NONE);

    let ssl_ctx = ssl_ctx.build();

    let ssl = Ssl::new(&ssl_ctx)?;

    let mut stream = tokio_openssl::SslStream::new(ssl, stream)?;
    Pin::new(&mut stream).connect().await?;
    Ok(stream)
}

async fn forward_to_backend<C, B>(
    initial_data: Vec<u8>,
    mut client: C,
    mut backend: B,
) -> Result<()>
where
    C: AsyncRead + AsyncWrite + Unpin,
    B: AsyncRead + AsyncWrite + Unpin,
{
    if let Err(e) = backend.write(&initial_data).await {
        let _ = client.write(http_response!("502 Bad Gateway")).await;
        return Err(format_err!("error writing initial data: {e}"));
    }
    drop(initial_data);

    let _ = io::copy_bidirectional(&mut client, &mut backend).await;
    Ok(())
}

async fn api_server(bind: impl Into<std::net::SocketAddr>) {
    use warp::Filter;
    let server = warp::get().map(|| warp::reply::json(&ctx().hosts.borrow().clone()));
    warp::serve(server).try_bind(bind).await;
}

struct KubeWatcher {
    client: Client,
    state: WatcherState,
    namespace: Option<String>,
    tx: watch::Sender<Arc<Hosts>>,
}
impl KubeWatcher {
    fn new(client: Client, namespace: Option<String>) -> (Self, HostsReceiver) {
        let (tx, cfg_rx) = watch::channel(Arc::new(Hosts::new()));

        (
            Self {
                client,
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
            None => WatcherStreams::all(&self.client),
            Some(ns) => WatcherStreams::namespaced(&self.client, ns.as_str()),
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

            // assemble proxy config
            let mut hosts = Hosts::new();

            for (key, ing) in &self.state.ingresses {
                for rule in &ing.rules {
                    let mut host_config = match hosts.get(&rule.host) {
                        Some(prev) => (**prev).clone(),
                        None => Default::default(),
                    };

                    if let Some(tls_secret) = rule.tls_secret.as_ref() {
                        let key = ObjectKey {
                            namespace: key.namespace.clone(),
                            name: tls_secret.clone(),
                        };
                        host_config.tls_key_cert = self.state.secrets.get(&key).cloned();
                        host_config.tls_secret = Some(key);
                    }

                    for m in &rule.matches {
                        let Some(endpoint) = m.endpoint(&key.namespace, ing.endpoint_opts.clone())
                        else {
                            continue;
                        };

                        use PathMatch::*;
                        match &m.path_match {
                            Exact(path) => {
                                host_config.exact_matches.insert(path.clone(), endpoint);
                            }
                            Prefix(path) => {
                                host_config.prefix_matches.insert(path.clone(), endpoint);
                            }
                            Any => {
                                host_config.any_match = Some(endpoint);
                            }
                        }
                    }

                    hosts.insert(rule.host.clone(), Arc::new(host_config));
                }
            }

            self.tx.send_replace(Arc::new(hosts));
        }
    }
}

type Stream<T> = Pin<Box<dyn futures::Stream<Item = watcher::Result<watcher::Event<T>>> + Send>>;

struct WatcherStreams {
    ing: Stream<networking::Ingress>,
    secrets: Stream<core::Secret>,
}
impl WatcherStreams {
    fn all(client: &Client) -> Self {
        let wcfg = watcher::Config::default();
        let sec_wcfg = wcfg.clone().fields("type=kubernetes.io/tls");
        Self {
            ing: watcher(Api::all(client.clone()), wcfg).boxed(),
            secrets: watcher(Api::all(client.clone()), sec_wcfg).boxed(),
        }
    }

    fn namespaced(client: &Client, ns: &str) -> Self {
        let wcfg = watcher::Config::default();
        let sec_wcfg = wcfg.clone().fields("type=kubernetes.io/tls");
        Self {
            ing: watcher(Api::namespaced(client.clone(), ns), wcfg).boxed(),
            secrets: watcher(Api::namespaced(client.clone(), ns), sec_wcfg).boxed(),
        }
    }
}

struct WatcherState {
    ingresses: Map<ObjectKey, Ingress>,
    ings_ready: bool,
    secrets: Map<ObjectKey, Arc<CertifiedKey>>,
    secrets_ready: bool,
}
impl WatcherState {
    fn new() -> Self {
        Self {
            ingresses: Map::new(),
            ings_ready: false,
            secrets: Map::new(),
            secrets_ready: false,
        }
    }

    fn is_ready(&self) -> bool {
        self.ings_ready && self.secrets_ready
    }

    fn clear(&mut self) {
        self.ingresses.clear();
        self.ings_ready = false;
        self.secrets.clear();
        self.secrets_ready = false;
    }

    async fn ingest_any_event(&mut self, streams: &mut WatcherStreams) -> eyre::Result<()> {
        tokio::select!(
          e = streams.ing.try_next() => {
              let e = e?.unwrap();
              trace!("got ing event: {e:?}");
              self.ings_ready = ingest_event::<Ingress, _>(&mut self.ingresses, e);
          },
          e = streams.secrets.try_next() => {
              let e = e?.unwrap();
              trace!("got secret event: {e:?}");
              self.ingest_secret_event(e);
          },
        );

        Ok(())
    }

    fn ingest_secret_event(&mut self, event: watcher::Event<core::Secret>) {
        use watcher::Event::*;
        self.secrets_ready = match event {
            Init => false,
            InitApply(sec) => {
                self.set_secret(sec);
                false
            }
            InitDone => true,
            Apply(sec) => {
                self.set_secret(sec);
                true
            }
            Delete(sec) => {
                self.remove_secret(sec);
                true
            }
        };
    }

    fn set_secret(&mut self, sec: core::Secret) {
        let key = ObjectKey::try_from(&sec.metadata).unwrap();

        let Some(data) = sec.data else {
            return;
        };
        let Some(cert) = data.get("tls.crt") else {
            return;
        };
        let Some(tls_key) = data.get("tls.key") else {
            return;
        };

        let Ok(ck) = CertifiedKey::from_pem(&tls_key.0, &cert.0)
            .inspect_err(|e| warn!("invalid (key, cert) in {key}: {e}"))
        else {
            return;
        };

        self.secrets.insert(key, Arc::new(ck));
    }
    fn remove_secret(&mut self, sec: core::Secret) {
        let key = ObjectKey::try_from(&sec.metadata).unwrap();
        self.secrets.remove(&key);
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

trait KeyValueFrom<V>: Sized {
    type Key: Ord;
    type Error;
    fn key_from(v: &V) -> Result<Self::Key, Self::Error>;
    fn value_from(v: &V) -> Result<Self, Self::Error>;
}

#[derive(Debug, serde::Serialize)]
struct Ingress {
    rules: Vec<IngressRule>,
    endpoint_opts: EndpointOptions,
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

        let get_opt = |k: &str| -> Option<&str> {
            let ann = ing.metadata.annotations.as_ref()?;
            let v = ann
                .get(&format!("ingress.kubernetes.io/{k}"))
                .or_else(|| ann.get(&format!("nginx.ingress.kubernetes.io/{k}")))?;
            Some(v.as_str())
        };

        Ok(Self {
            rules,
            endpoint_opts: EndpointOptions {
                secure_backends: get_opt("secure-backends") == Some("true"),
                ssl_redirect: get_opt("secure-backends") == Some("true"),
                http2: get_opt("http2") == Some("true"),
            },
        })
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
            matches: (rule.http.as_ref())
                .map(|http| {
                    (http.paths.iter())
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

    fn endpoint(&self, namespace: &str, opts: EndpointOptions) -> Option<Endpoint> {
        let Some(backend) = self.backend.as_ref() else {
            return None;
        };

        Some(Endpoint {
            namespace: namespace.into(),
            service: backend.service.clone(),
            port: backend.port.clone(),
            opts,
        })
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
enum PathMatch {
    Exact(String),
    Prefix(String),
    Any,
}
