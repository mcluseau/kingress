use eyre::Result;
use log::{debug, trace, warn};
use std::net::SocketAddr;
use std::num::NonZero;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

use super::{Endpoint, Resolver};

pub struct Builder {
    pub size: usize,
    pub expiry_secs: u64,
    pub negative_expiry_secs: u64,
    pub resolver: Resolver,
}
impl Builder {
    pub fn build(self) -> Cache {
        Cache {
            resolver: self.resolver,
            lru: NonZero::new(self.size).map(|s| Mutex::new(lru::LruCache::new(s))),
            expiry: Duration::from_secs(self.expiry_secs),
            negative_expiry: Duration::from_secs(self.negative_expiry_secs),
        }
    }
}

pub struct Cache {
    resolver: Resolver,
    lru: Option<Mutex<lru::LruCache<String, Arc<Mutex<Option<ResolveResult>>>>>>,
    expiry: Duration,
    negative_expiry: Duration,
}

impl Cache {
    pub async fn resolve(&self, ep: &Endpoint) -> Vec<SocketAddr> {
        let Some(ref lru) = self.lru else {
            return self.resolve_no_cache(ep).await.result();
        };

        let key = ep.to_string();

        let cache_entry = (lru.lock().await)
            .get_or_insert(key, || Arc::new(Mutex::new(None)))
            .clone();

        let mut cache_entry = cache_entry.lock().await;

        if let Some(result) = cache_entry.as_ref() {
            if self.is_expired(result) {
                trace!("cached result expired: {result:?}");
            } else {
                trace!("using cached result: {result:?}");
                return result.result();
            }
        }

        let result = self.resolve_no_cache(ep).await;
        let ret = result.result();

        // cache the result
        debug!("caching result: {ep} -> {result:?}");
        *cache_entry = Some(result);

        ret
    }

    async fn resolve_no_cache(&self, ep: &Endpoint) -> ResolveResult {
        let result = self.resolver.resolve(ep).await;

        if let Err(ref e) = result {
            warn!("failed to resolve {ep}: {e}");
        }

        ResolveResult::new(result)
    }

    fn is_expired(&self, result: &ResolveResult) -> bool {
        let expiry = match result {
            ResolveResult::Ok { .. } => self.expiry,
            ResolveResult::Failed { .. } => self.negative_expiry,
        };
        result.age() > expiry
    }
}

enum ResolveResult {
    Ok {
        cached_at: Instant,
        result: Vec<SocketAddr>,
    },
    Failed {
        cached_at: Instant,
    },
}
impl ResolveResult {
    fn new(result: Result<Vec<SocketAddr>>) -> Self {
        let cached_at = Instant::now();
        match result {
            Ok(v) => Self::Ok {
                cached_at,
                result: v,
            },
            Err(_) => Self::Failed { cached_at },
        }
    }

    fn cached_at(&self) -> Instant {
        match self {
            Self::Ok { cached_at, .. } => *cached_at,
            Self::Failed { cached_at } => *cached_at,
        }
    }

    fn age(&self) -> Duration {
        Instant::now() - self.cached_at()
    }

    fn result(&self) -> Vec<SocketAddr> {
        match self {
            Self::Ok { result, .. } => result.clone(),
            Self::Failed { .. } => vec![],
        }
    }
}
impl std::fmt::Debug for ResolveResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        let age = self.age().as_millis();
        match self {
            Self::Ok { result, .. } => write!(f, "Ok({age}ms ago, {result:?})"),
            Self::Failed { .. } => write!(f, "Failed({age}ms ago)"),
        }
    }
}
