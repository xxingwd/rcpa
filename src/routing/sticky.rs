//! Sticky session tracking
//! Maps session key -> provider name with TTL expiry

use std::sync::Arc;
use std::time::{Duration, Instant};

/// A sticky session target with TTL
#[derive(Debug, Clone)]
pub struct StickyTarget {
    pub provider: String,
    pub expires_at: Instant,
}

/// Sticky session table — cheap clone of inner Arc
#[derive(Clone)]
pub struct StickySessions {
    inner: Arc<dashmap::DashMap<String, StickyTarget>>,
    ttl: Duration,
}

impl StickySessions {
    pub fn new(ttl_secs: u64) -> Self {
        Self {
            inner: Arc::new(dashmap::DashMap::new()),
            ttl: Duration::from_secs(ttl_secs),
        }
    }

    /// Get the provider for a session key, if still valid
    pub fn get(&self, key: &str) -> Option<String> {
        if let Some(entry) = self.inner.get(key) {
            if entry.expires_at > Instant::now() {
                return Some(entry.provider.clone());
            }
            // Expired — drop it
            drop(entry);
            self.inner.remove(key);
        }
        None
    }

    /// Set a session -> provider mapping
    pub fn set(&self, key: String, provider: String) {
        self.inner.insert(
            key,
            StickyTarget {
                provider,
                expires_at: Instant::now() + self.ttl,
            },
        );
    }

    pub fn remove(&self, key: &str) {
        self.inner.remove(key);
    }

    /// Remove expired entries (called periodically)
    pub fn cleanup(&self) {
        let now = Instant::now();
        self.inner.retain(|_, v| v.expires_at > now);
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}
