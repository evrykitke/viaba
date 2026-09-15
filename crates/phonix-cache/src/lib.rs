//! Tenant-scoped Redis cache.
//!
//! Keys use `<prefix>:<tenant>:<key>`. With `redis.fail_open`, failures are
//! treated as cache misses.

use std::time::Duration;

use phonix_config::RedisConfig;
use phonix_core::TenantSlug;
use redis::aio::{ConnectionManager, ConnectionManagerConfig};
use redis::{AsyncCommands, Client, ConnectionAddr, IntoConnectionInfo, RedisConnectionInfo};
use secrecy::ExposeSecret;
use serde::Serialize;
use serde::de::DeserializeOwned;

#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    #[error("could not connect to redis at {addr}: {source}")]
    Connect {
        addr: String,
        #[source]
        source: redis::RedisError,
    },

    #[error("redis command failed: {0}")]
    Command(#[from] redis::RedisError),

    #[error("could not serialise value for key '{key}': {source}")]
    Serialize {
        key: String,
        #[source]
        source: serde_json::Error,
    },
}

/// A connected Redis cache, or a disabled no-op.
#[derive(Clone)]
pub struct Cache {
    inner: Option<ConnectionManager>,
    key_prefix: String,
    default_ttl: Duration,
    fail_open: bool,
}

impl Cache {
    /// Connects using the supplied configuration, or returns a disabled cache.
    pub async fn connect(cfg: &RedisConfig) -> Result<Self, CacheError> {
        if !cfg.enabled {
            tracing::info!("redis is disabled; cache operations will be no-ops");
            return Ok(Self::disabled(cfg));
        }

        let addr = if cfg.use_tls {
            ConnectionAddr::TcpTls {
                host: cfg.host.clone(),
                port: cfg.port,
                insecure: false,
                tls_params: None,
            }
        } else {
            ConnectionAddr::Tcp(cfg.host.clone(), cfg.port)
        };
        let display_addr = format!("{}:{}", cfg.host, cfg.port);

        // Build settings directly so passwords need no URL encoding.
        let mut redis_settings = RedisConnectionInfo::default().set_db(cfg.database as i64);
        if !cfg.username.trim().is_empty() {
            redis_settings = redis_settings.set_username(&cfg.username);
        }
        if !cfg.password.expose_secret().is_empty() {
            redis_settings = redis_settings.set_password(cfg.password.expose_secret());
        }

        let info = addr
            .into_connection_info()
            .map_err(|source| CacheError::Connect {
                addr: display_addr.clone(),
                source,
            })?
            .set_redis_settings(redis_settings);

        let client = Client::open(info).map_err(|source| CacheError::Connect {
            addr: display_addr.clone(),
            source,
        })?;

        // The connection manager reconnects automatically.
        let manager_config = ConnectionManagerConfig::new()
            .set_connection_timeout(Some(Duration::from_secs(cfg.connect_timeout_secs)))
            .set_response_timeout(Some(Duration::from_secs(cfg.response_timeout_secs)));

        let manager = ConnectionManager::new_with_config(client, manager_config)
            .await
            .map_err(|source| CacheError::Connect {
                addr: display_addr.clone(),
                source,
            })?;

        tracing::info!(addr = %display_addr, db = cfg.database, "connected to redis");

        Ok(Self {
            inner: Some(manager),
            key_prefix: cfg.key_prefix.clone(),
            default_ttl: Duration::from_secs(cfg.default_ttl_secs),
            fail_open: cfg.fail_open,
        })
    }

    /// A cache that silently does nothing. Used when Redis is disabled.
    pub fn disabled(cfg: &RedisConfig) -> Self {
        Self {
            inner: None,
            key_prefix: cfg.key_prefix.clone(),
            default_ttl: Duration::from_secs(cfg.default_ttl_secs),
            fail_open: true,
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.inner.is_some()
    }

    /// Scope the cache to one tenant.
    pub fn for_tenant(&self, slug: &TenantSlug) -> TenantCache {
        TenantCache {
            cache: self.clone(),
            namespace: format!("{}:{}", self.key_prefix, slug),
        }
    }

    /// `PING`, for the readiness endpoint.
    pub async fn ping(&self) -> Result<(), CacheError> {
        let Some(mut conn) = self.inner.clone() else {
            return Ok(());
        };
        redis::cmd("PING").query_async::<()>(&mut conn).await?;
        Ok(())
    }

    /// Decide what a failed command means, given `fail_open`.
    fn absorb<T>(&self, key: &str, err: redis::RedisError) -> Result<Option<T>, CacheError> {
        if self.fail_open {
            tracing::warn!(key, error = %err, "cache error treated as a miss");
            Ok(None)
        } else {
            Err(CacheError::Command(err))
        }
    }
}

/// Cache handle bound to a single tenant's namespace.
#[derive(Clone)]
pub struct TenantCache {
    cache: Cache,
    namespace: String,
}

impl TenantCache {
    /// Fully-qualified key: `<prefix>:<tenant>:<key>`.
    pub fn key(&self, key: &str) -> String {
        format!("{}:{}", self.namespace, key)
    }

    /// Fetches and deserialises a value; stale values are removed as misses.
    pub async fn get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>, CacheError> {
        let Some(mut conn) = self.cache.inner.clone() else {
            return Ok(None);
        };
        let full_key = self.key(key);

        let raw: Option<String> = match conn.get(&full_key).await {
            Ok(value) => value,
            Err(err) => return self.cache.absorb(&full_key, err),
        };

        let Some(raw) = raw else {
            return Ok(None);
        };

        match serde_json::from_str(&raw) {
            Ok(value) => Ok(Some(value)),
            Err(err) => {
                tracing::warn!(
                    key = %full_key,
                    error = %err,
                    "discarding cache entry that no longer matches its type"
                );
                let _ = self.delete(key).await;
                Ok(None)
            }
        }
    }

    /// Store a value with the configured default TTL.
    pub async fn set<T: Serialize>(&self, key: &str, value: &T) -> Result<(), CacheError> {
        self.set_with_ttl(key, value, self.cache.default_ttl).await
    }

    /// Stores a value with an explicit TTL; zero uses the default TTL.
    pub async fn set_with_ttl<T: Serialize>(
        &self,
        key: &str,
        value: &T,
        ttl: Duration,
    ) -> Result<(), CacheError> {
        let Some(mut conn) = self.cache.inner.clone() else {
            return Ok(());
        };
        let full_key = self.key(key);

        let payload = serde_json::to_string(value).map_err(|source| CacheError::Serialize {
            key: full_key.clone(),
            source,
        })?;

        let ttl = if ttl.is_zero() {
            self.cache.default_ttl
        } else {
            ttl
        };

        let result: Result<(), _> = conn.set_ex(&full_key, payload, ttl.as_secs()).await;

        if let Err(err) = result {
            if self.cache.fail_open {
                tracing::warn!(key = %full_key, error = %err, "cache write failed; continuing");
            } else {
                return Err(CacheError::Command(err));
            }
        }
        Ok(())
    }

    /// Remove a key. Deleting a missing key is not an error.
    pub async fn delete(&self, key: &str) -> Result<(), CacheError> {
        let Some(mut conn) = self.cache.inner.clone() else {
            return Ok(());
        };
        let full_key = self.key(key);

        let result: Result<(), _> = conn.del(&full_key).await;

        if let Err(err) = result {
            if self.cache.fail_open {
                tracing::warn!(key = %full_key, error = %err, "cache delete failed; continuing");
            } else {
                return Err(CacheError::Command(err));
            }
        }
        Ok(())
    }

    /// Reads through the cache, computing and storing a value on a miss.
    pub async fn get_or_insert_with<T, F, Fut, E>(&self, key: &str, compute: F) -> Result<T, E>
    where
        T: Serialize + DeserializeOwned,
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T, E>>,
    {
        // On a cache error, compute the value instead.
        if let Ok(Some(hit)) = self.get::<T>(key).await {
            tracing::trace!(key = %self.key(key), "cache hit");
            return Ok(hit);
        }

        tracing::trace!(key = %self.key(key), "cache miss");
        let value = compute().await?;
        let _ = self.set(key, &value).await;
        Ok(value)
    }
}
