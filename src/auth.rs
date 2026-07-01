use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use axum::http::HeaderMap;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use crate::{
    config::ClientConfig,
    error::{ErrorKind, ProxyError, Result},
};

pub struct AuthRegistry {
    clients: Vec<Arc<ClientIdentity>>,
}

pub struct ClientIdentity {
    pub id: String,
    key_hash: [u8; 32],
    allowed_routes: Vec<String>,
    limiter: Mutex<VecDeque<Instant>>,
    requests_per_minute: usize,
    concurrency: Arc<Semaphore>,
}

pub struct ClientPermit {
    pub identity: Arc<ClientIdentity>,
    _permit: OwnedSemaphorePermit,
}

impl AuthRegistry {
    pub fn new(configs: &[ClientConfig]) -> Result<Self> {
        let mut clients = Vec::with_capacity(configs.len());
        for config in configs {
            let key = config.key.resolve()?;
            if key.len() < 24 {
                return Err(ProxyError::new(
                    ErrorKind::Internal,
                    format!("client {} key must be at least 24 bytes", config.id),
                ));
            }
            clients.push(Arc::new(ClientIdentity {
                id: config.id.clone(),
                key_hash: hash_key(&key),
                allowed_routes: config.allowed_routes.clone(),
                limiter: Mutex::new(VecDeque::new()),
                requests_per_minute: config.requests_per_minute as usize,
                concurrency: Arc::new(Semaphore::new(config.concurrent_requests as usize)),
            }));
        }
        Ok(Self { clients })
    }

    pub fn authenticate(&self, headers: &HeaderMap) -> Result<Arc<ClientIdentity>> {
        let key = extract_key(headers)
            .ok_or_else(|| ProxyError::new(ErrorKind::Authentication, "missing API key"))?;
        let candidate = hash_key(key);
        self.clients
            .iter()
            .find(|client| bool::from(client.key_hash.ct_eq(&candidate)))
            .cloned()
            .ok_or_else(|| ProxyError::new(ErrorKind::Authentication, "invalid API key"))
    }
}

impl ClientIdentity {
    pub fn allows_route(&self, route: &str) -> bool {
        self.allowed_routes.is_empty() || self.allowed_routes.iter().any(|allowed| allowed == route)
    }

    pub fn acquire(self: &Arc<Self>) -> Result<ClientPermit> {
        let now = Instant::now();
        let cutoff = now - Duration::from_secs(60);
        {
            let mut requests = self.limiter.lock().expect("client rate limiter poisoned");
            while requests
                .front()
                .is_some_and(|timestamp| *timestamp <= cutoff)
            {
                requests.pop_front();
            }
            if requests.len() >= self.requests_per_minute {
                let mut error =
                    ProxyError::new(ErrorKind::RateLimit, "client request rate exceeded");
                error.retry_after_seconds = Some(1);
                return Err(error);
            }
            requests.push_back(now);
        }

        let permit =
            self.concurrency.clone().try_acquire_owned().map_err(|_| {
                ProxyError::new(ErrorKind::RateLimit, "client concurrency exceeded")
            })?;
        Ok(ClientPermit {
            identity: self.clone(),
            _permit: permit,
        })
    }
}

fn extract_key(headers: &HeaderMap) -> Option<&str> {
    if let Some(value) = headers
        .get("x-api-key")
        .and_then(|value| value.to_str().ok())
    {
        return Some(value);
    }
    headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
}

fn hash_key(key: &str) -> [u8; 32] {
    Sha256::digest(key.as_bytes()).into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SecretRef;

    #[test]
    fn accepts_both_supported_headers() {
        std::env::set_var("TEST_PROXY_KEY", "high-entropy-test-key-1234");
        let registry = AuthRegistry::new(&[ClientConfig {
            id: "test".into(),
            key: SecretRef::Env {
                env: "TEST_PROXY_KEY".into(),
            },
            allowed_routes: vec![],
            requests_per_minute: 10,
            concurrent_requests: 2,
        }])
        .unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(
            "authorization",
            "Bearer high-entropy-test-key-1234".parse().unwrap(),
        );
        assert_eq!(registry.authenticate(&headers).unwrap().id, "test");
        headers.clear();
        headers.insert("x-api-key", "high-entropy-test-key-1234".parse().unwrap());
        assert_eq!(registry.authenticate(&headers).unwrap().id, "test");
    }
}
