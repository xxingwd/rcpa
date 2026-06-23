//! Sticky session tracking
//! Maps session key -> provider name with TTL expiry

use std::sync::Arc;
use std::time::{Duration, Instant};

/// A sticky session target with TTL
#[derive(Debug, Clone)]
pub struct StickyTarget {
    pub provider: String,
    pub expires_at: Instant,
    pub pinned_at: Instant,
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

    /// Get the provider for a session key, refreshing the TTL when still valid.
    pub fn get(&self, key: &str) -> Option<String> {
        self.get_with_ttl(key, self.ttl)
    }

    /// Get the provider for a session key, refreshing with a caller supplied TTL.
    pub fn get_with_ttl(&self, key: &str, ttl: Duration) -> Option<String> {
        self.get_with_pinned_at(key, ttl)
            .map(|(provider, _)| provider)
    }

    /// Get the provider and its pinned time for a session key, refreshing with a caller supplied TTL.
    pub fn get_with_pinned_at(&self, key: &str, ttl: Duration) -> Option<(String, Instant)> {
        if let Some(mut entry) = self.inner.get_mut(key) {
            let now = Instant::now();
            if entry.expires_at > now {
                entry.expires_at = now + ttl;
                return Some((entry.provider.clone(), entry.pinned_at));
            }
            // Expired — drop it
            drop(entry);
            self.inner.remove(key);
        }
        None
    }

    /// Set a session -> provider mapping
    pub fn set(&self, key: String, provider: String) {
        self.set_with_ttl(key, provider, self.ttl);
    }

    /// Set a session -> provider mapping with a caller supplied TTL.
    /// Preserves the pinned_at time if the provider name is unchanged.
    pub fn set_with_ttl(&self, key: String, provider: String, ttl: Duration) {
        let now = Instant::now();
        self.inner
            .entry(key)
            .and_modify(|target| {
                if target.provider != provider {
                    target.provider = provider.clone();
                    target.pinned_at = now;
                }
                target.expires_at = now + ttl;
            })
            .or_insert(StickyTarget {
                provider,
                expires_at: now + ttl,
                pinned_at: now,
            });
    }

    pub fn remove(&self, key: &str) {
        self.inner.remove(key);
    }

    /// Remove all sessions pinned to a provider that is no longer usable.
    pub fn invalidate_provider(&self, provider: &str) {
        self.inner.retain(|_, target| target.provider != provider);
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

#[cfg(test)]
mod tests {
    use super::StickySessions;
    use std::time::Duration;

    #[test]
    fn get_refreshes_ttl() {
        let sessions = StickySessions::new(60);
        sessions.set("session-a".to_string(), "provider-a".to_string());
        let first_expiry = sessions.inner.get("session-a").unwrap().expires_at;

        std::thread::sleep(Duration::from_millis(2));

        assert_eq!(sessions.get("session-a").as_deref(), Some("provider-a"));
        let refreshed_expiry = sessions.inner.get("session-a").unwrap().expires_at;
        assert!(refreshed_expiry > first_expiry);
    }

    #[test]
    fn get_removes_expired_entry() {
        let sessions = StickySessions::new(0);
        sessions.set("session-a".to_string(), "provider-a".to_string());

        assert_eq!(sessions.get("session-a"), None);
        assert!(sessions.is_empty());
    }

    #[test]
    fn invalidate_provider_removes_only_matching_sessions() {
        let sessions = StickySessions::new(60);
        sessions.set("session-a".to_string(), "provider-a".to_string());
        sessions.set("session-b".to_string(), "provider-b".to_string());
        sessions.set("session-c".to_string(), "provider-a".to_string());

        sessions.invalidate_provider("provider-a");

        assert_eq!(sessions.get("session-a"), None);
        assert_eq!(sessions.get("session-b").as_deref(), Some("provider-b"));
        assert_eq!(sessions.get("session-c"), None);
    }
}
