pub mod model_config;
pub mod sticky;
pub mod strategy;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use crate::config::AppConfig;
use crate::config_service::ConfigSnapshot;
use crate::error::AppError;
use crate::protocol::common::Operation;
use crate::server::AppState;

/// Routing strategy selector
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Strategy {
    RoundRobin,
    WeightedRoundRobin,
    StickySession,
    LeastConnections,
}

impl std::str::FromStr for Strategy {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "round_robin" => Ok(Strategy::RoundRobin),
            "weighted_round_robin" => Ok(Strategy::WeightedRoundRobin),
            "sticky_session" => Ok(Strategy::StickySession),
            "least_connections" => Ok(Strategy::LeastConnections),
            other => Err(format!("Unknown routing strategy '{}'", other)),
        }
    }
}

/// Health status for a provider
pub struct ProviderHealth {
    pub consecutive_failures: AtomicUsize,
    pub last_failure: parking_lot::Mutex<Option<std::time::Instant>>,
    pub is_healthy: AtomicBool,
}

impl ProviderHealth {
    fn new() -> Self {
        Self {
            consecutive_failures: AtomicUsize::new(0),
            last_failure: parking_lot::Mutex::new(None),
            is_healthy: AtomicBool::new(true),
        }
    }
}

/// Tracks per-model routing state
pub struct ModelRouter {
    strategy: Strategy,
    sticky_enabled: bool,
    /// Round-robin counter per model
    rr_counters: Arc<dashmap::DashMap<String, AtomicUsize>>,
    /// Provider health tracking
    health: Arc<dashmap::DashMap<String, ProviderHealth>>,
}

impl ModelRouter {
    pub fn new(config: &AppConfig) -> anyhow::Result<Self> {
        let strategy: Strategy = config
            .routing
            .strategy
            .parse()
            .map_err(|err: String| anyhow::anyhow!(err))?;
        let health = Arc::new(dashmap::DashMap::new());
        for provider in config
            .providers
            .iter()
            .filter(|provider| provider.is_enabled())
        {
            health.insert(provider.name.clone(), ProviderHealth::new());
        }
        Ok(Self {
            strategy,
            sticky_enabled: config.routing.sticky.enabled,
            rr_counters: Arc::new(dashmap::DashMap::new()),
            health,
        })
    }

    /// Route a model request to a provider name.
    /// `session_key` is used for sticky sessions (typically "api_key:model")
    pub fn route(
        &self,
        model: &str,
        _operation: &Operation,
        state: &AppState,
        snapshot: &ConfigSnapshot,
        session_key: &str,
    ) -> Result<String, AppError> {
        let expected_protocol = _operation.provider_protocol();
        let all_providers: Vec<String> = snapshot
            .providers_for_model(model)
            .into_iter()
            .filter(|provider| snapshot.provider_protocol(provider) == Some(expected_protocol))
            .collect();

        if all_providers.is_empty() {
            return Err(AppError::ModelNotFound(model.to_string()));
        }

        self.ensure_provider_health_entries(&all_providers);
        self.check_recovery();

        // Check sticky session first
        if self.sticky_enabled {
            if let Some(provider) = state.sticky_sessions.get(session_key) {
                if snapshot.enabled_provider_can_serve_model(&provider, model)
                    && self.is_provider_healthy(&provider)
                {
                    return Ok(provider);
                }
                state.sticky_sessions.remove(session_key);
            }
        }

        // Filter out unhealthy providers.
        let healthy_providers: Vec<String> = all_providers
            .iter()
            .filter(|provider| self.is_provider_healthy(provider))
            .cloned()
            .collect();

        if healthy_providers.is_empty() {
            return Err(AppError::NoProviderAvailable(model.to_string()));
        }

        let provider_name = match self.strategy {
            Strategy::RoundRobin => self.round_robin(model, &healthy_providers),
            Strategy::WeightedRoundRobin => {
                self.weighted_round_robin(model, &healthy_providers, &snapshot.provider_weights)
            }
            Strategy::StickySession => self.round_robin(model, &healthy_providers),
            Strategy::LeastConnections => self.least_connections(&healthy_providers, snapshot),
        };

        Ok(provider_name)
    }

    pub fn ensure_provider_health_entry(&self, provider: &str) {
        self.health
            .entry(provider.to_string())
            .or_insert_with(ProviderHealth::new);
    }

    fn ensure_provider_health_entries(&self, providers: &[String]) {
        for provider in providers {
            self.ensure_provider_health_entry(provider);
        }
    }

    fn round_robin(&self, model: &str, providers: &[String]) -> String {
        let counter = self
            .rr_counters
            .entry(model.to_string())
            .or_insert_with(|| AtomicUsize::new(0));

        let idx = counter.fetch_add(1, Ordering::Relaxed) % providers.len();
        providers[idx].clone()
    }

    fn weighted_round_robin(
        &self,
        model: &str,
        providers: &[String],
        weights: &HashMap<String, u32>,
    ) -> String {
        // Build a weighted selection: if provider A has weight 10 and B has weight 5,
        // the effective pool ratio is 2:1
        let total_weight: u32 = providers
            .iter()
            .map(|provider| weights.get(provider).copied().unwrap_or(10))
            .sum();

        if total_weight == 0 {
            return self.round_robin(model, providers);
        }

        let counter = self
            .rr_counters
            .entry(model.to_string())
            .or_insert_with(|| AtomicUsize::new(0));
        let idx = counter.fetch_add(1, Ordering::Relaxed) % total_weight as usize;

        let mut cumulative = 0u32;
        for provider in providers {
            cumulative += weights.get(provider).copied().unwrap_or(10);
            if idx < cumulative as usize {
                return provider.clone();
            }
        }
        providers[0].clone()
    }

    fn least_connections(&self, providers: &[String], snapshot: &ConfigSnapshot) -> String {
        providers
            .iter()
            .min_by_key(|name| snapshot.registry.connection_count(name))
            .cloned()
            .unwrap_or_else(|| providers[0].clone())
    }

    // --- Provider health tracking ---

    /// Record a provider failure. Marks unhealthy after > 3 consecutive failures.
    pub fn record_provider_failure(&self, provider: &str) {
        let entry = self
            .health
            .entry(provider.to_string())
            .or_insert_with(ProviderHealth::new);
        let failures = entry.consecutive_failures.fetch_add(1, Ordering::Relaxed) + 1;
        *entry.last_failure.lock() = Some(std::time::Instant::now());
        if failures > 3 {
            entry.is_healthy.store(false, Ordering::Relaxed);
        }
    }

    /// Record a provider success. Resets consecutive failures and marks healthy.
    pub fn record_provider_success(&self, provider: &str) {
        let entry = self
            .health
            .entry(provider.to_string())
            .or_insert_with(ProviderHealth::new);
        entry.consecutive_failures.store(0, Ordering::Relaxed);
        entry.is_healthy.store(true, Ordering::Relaxed);
    }

    /// Check if a provider is considered healthy.
    pub fn is_provider_healthy(&self, provider: &str) -> bool {
        self.health
            .get(provider)
            .map(|h| h.is_healthy.load(Ordering::Relaxed))
            .unwrap_or(false)
    }

    /// Check for recovery: mark providers healthy again if last failure > 30s ago.
    pub fn check_recovery(&self) {
        let now = std::time::Instant::now();
        for entry in self.health.iter() {
            if !entry.is_healthy.load(Ordering::Relaxed) {
                let last = *entry.last_failure.lock();
                if let Some(last_failure_time) = last {
                    if now.duration_since(last_failure_time).as_secs() > 30 {
                        entry.is_healthy.store(true, Ordering::Relaxed);
                        entry.consecutive_failures.store(0, Ordering::Relaxed);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// Helper: create a minimal ModelRouter with given providers and weights
    fn make_router(strategy: Strategy, model: &str, providers: &[(&str, u32)]) -> ModelRouter {
        let _ = (model, providers);
        ModelRouter {
            strategy,
            sticky_enabled: false,
            rr_counters: Arc::new(dashmap::DashMap::new()),
            health: Arc::new(dashmap::DashMap::new()),
        }
    }

    #[test]
    fn test_weighted_round_robin_distribution() {
        // Provider A weight=20, B weight=10, ratio 2:1.
        let router = make_router(
            Strategy::WeightedRoundRobin,
            "gpt-4o",
            &[("provider-a", 20), ("provider-b", 10)],
        );

        let mut counts: HashMap<String, usize> = HashMap::new();
        let providers = vec!["provider-a".to_string(), "provider-b".to_string()];
        let weights = HashMap::from([
            ("provider-a".to_string(), 20),
            ("provider-b".to_string(), 10),
        ]);
        for _ in 0..30 {
            let picked = router.weighted_round_robin("gpt-4o", &providers, &weights);
            *counts.entry(picked).or_insert(0) += 1;
        }
        // 30 requests over total weight 30: A gets 20, B gets 10
        assert_eq!(counts.get("provider-a").copied().unwrap_or(0), 20);
        assert_eq!(counts.get("provider-b").copied().unwrap_or(0), 10);
    }

    #[test]
    fn test_weighted_round_robin_equal_weights() {
        let router = make_router(
            Strategy::WeightedRoundRobin,
            "gpt-4o",
            &[("a", 10), ("b", 10)],
        );

        let providers = vec!["a".to_string(), "b".to_string()];
        let weights = HashMap::from([("a".to_string(), 10), ("b".to_string(), 10)]);
        let mut counts: HashMap<String, usize> = HashMap::new();
        for _ in 0..20 {
            let picked = router.weighted_round_robin("gpt-4o", &providers, &weights);
            *counts.entry(picked).or_insert(0) += 1;
        }
        assert_eq!(counts.get("a").copied().unwrap_or(0), 10);
        assert_eq!(counts.get("b").copied().unwrap_or(0), 10);
    }

    #[test]
    fn test_provider_health_tracking() {
        let router = make_router(
            Strategy::RoundRobin,
            "gpt-4o",
            &[("healthy", 10), ("unhealthy", 10)],
        );

        // Initially all healthy
        router.ensure_provider_health_entries(&["healthy".to_string(), "unhealthy".to_string()]);
        assert!(router.is_provider_healthy("healthy"));
        assert!(router.is_provider_healthy("unhealthy"));
        assert!(!router.is_provider_healthy("unknown-provider"));

        // Record 4 failures on "unhealthy" → becomes unhealthy (threshold > 3)
        for _ in 0..4 {
            router.record_provider_failure("unhealthy");
        }
        assert!(!router.is_provider_healthy("unhealthy"));
        assert!(router.is_provider_healthy("healthy"));

        // Record success resets health
        router.record_provider_success("unhealthy");
        assert!(router.is_provider_healthy("unhealthy"));
    }

    #[test]
    fn test_provider_health_not_triggered_below_threshold() {
        let router = make_router(Strategy::RoundRobin, "gpt-4o", &[("provider", 10)]);

        // 3 failures should NOT trigger unhealthy (threshold is > 3)
        for _ in 0..3 {
            router.record_provider_failure("provider");
        }
        assert!(router.is_provider_healthy("provider"));

        // 4th failure triggers unhealthy
        router.record_provider_failure("provider");
        assert!(!router.is_provider_healthy("provider"));
    }

    #[test]
    fn test_strategy_from_str() {
        assert_eq!(
            "round_robin".parse::<Strategy>().unwrap(),
            Strategy::RoundRobin
        );
        assert_eq!(
            "weighted_round_robin".parse::<Strategy>().unwrap(),
            Strategy::WeightedRoundRobin
        );
        assert_eq!(
            "sticky_session".parse::<Strategy>().unwrap(),
            Strategy::StickySession
        );
        assert_eq!(
            "least_connections".parse::<Strategy>().unwrap(),
            Strategy::LeastConnections
        );
        assert!("unknown".parse::<Strategy>().is_err());
    }

    #[test]
    fn test_round_robin_cycles() {
        let router = make_router(
            Strategy::RoundRobin,
            "gpt-4o",
            &[("a", 10), ("b", 10), ("c", 10)],
        );

        let providers = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let results: Vec<String> = (0..6)
            .map(|_| router.round_robin("gpt-4o", &providers))
            .collect();
        assert_eq!(results, vec!["a", "b", "c", "a", "b", "c"]);
    }
}
