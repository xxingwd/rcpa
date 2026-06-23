pub mod sticky;

use std::collections::{HashMap, HashSet};
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
    LeastConnections,
    PriorityWeightedLeastConn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RouteSelectionReason {
    Sticky,
    Healthy,
    Degraded,
}

impl RouteSelectionReason {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            RouteSelectionReason::Sticky => "sticky",
            RouteSelectionReason::Healthy => "healthy",
            RouteSelectionReason::Degraded => "degraded",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RouteDecision {
    pub provider_name: String,
    pub selection_reason: RouteSelectionReason,
    pub sticky_hit: bool,
}

impl std::str::FromStr for Strategy {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "round_robin" => Ok(Strategy::RoundRobin),
            "weighted_round_robin" => Ok(Strategy::WeightedRoundRobin),
            "least_connections" => Ok(Strategy::LeastConnections),
            "priority_weighted_least_conn" => Ok(Strategy::PriorityWeightedLeastConn),
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
    /// Round-robin counter per model
    rr_counters: Arc<dashmap::DashMap<String, AtomicUsize>>,
    /// Provider health tracking
    health: Arc<dashmap::DashMap<String, ProviderHealth>>,
}

impl ModelRouter {
    pub fn new(config: &AppConfig) -> anyhow::Result<Self> {
        let health = Arc::new(dashmap::DashMap::new());
        for provider in config
            .providers
            .iter()
            .filter(|provider| provider.is_enabled())
        {
            health.insert(provider.name.clone(), ProviderHealth::new());
        }
        Ok(Self {
            rr_counters: Arc::new(dashmap::DashMap::new()),
            health,
        })
    }

    /// Route a model request to a provider name.
    /// `session_key` is used for sticky sessions when routing.sticky.enabled is true.
    pub fn route(
        &self,
        model: &str,
        operation: &Operation,
        state: &AppState,
        snapshot: &ConfigSnapshot,
        session_key: &str,
    ) -> Result<String, AppError> {
        self.route_decision_with_exclusions(model, operation, state, snapshot, session_key, None)
            .map(|decision| decision.provider_name)
    }

    pub(crate) fn route_decision_with_exclusions(
        &self,
        model: &str,
        operation: &Operation,
        state: &AppState,
        snapshot: &ConfigSnapshot,
        session_key: &str,
        excluded_providers: Option<&HashSet<String>>,
    ) -> Result<RouteDecision, AppError> {
        let expected_protocol = operation.provider_protocol();
        let all_providers: Vec<String> = snapshot
            .providers_for_model(model)
            .into_iter()
            .filter(|provider| snapshot.provider_supports_protocol(provider, expected_protocol))
            .collect();

        if all_providers.is_empty() {
            return Err(AppError::ModelNotFound(model.to_string()));
        }

        self.ensure_provider_health_entries(&all_providers);
        self.check_recovery();

        let sticky_config = &snapshot.config.routing.sticky;
        if sticky_config.enabled {
            let ttl = std::time::Duration::from_secs(sticky_config.ttl_secs);
            if let Some((provider, pinned_at)) =
                state.sticky_sessions.get_with_pinned_at(session_key, ttl)
            {
                let mut is_valid_sticky = !is_excluded(excluded_providers, &provider)
                    && snapshot.enabled_provider_can_serve_model(&provider, model)
                    && snapshot.provider_supports_protocol(&provider, expected_protocol)
                    && self.is_provider_healthy(&provider);

                if is_valid_sticky {
                    let fallback_retry_interval =
                        sticky_config.fallback_retry_interval_secs.unwrap_or(60);
                    if pinned_at.elapsed().as_secs() >= fallback_retry_interval {
                        // Find the highest priority among all healthy, enabled providers for this model & protocol
                        let mut healthy_priorities: Vec<i64> = all_providers
                            .iter()
                            .filter(|p| self.is_provider_healthy(p))
                            .map(|p| {
                                snapshot
                                    .find_provider(p)
                                    .map(|p_cfg| p_cfg.priority)
                                    .unwrap_or(i64::MAX)
                            })
                            .collect();
                        healthy_priorities.sort_unstable();

                        if let Some(&highest_priority) = healthy_priorities.first() {
                            let sticky_priority = snapshot
                                .find_provider(&provider)
                                .map(|p_cfg| p_cfg.priority)
                                .unwrap_or(i64::MAX);

                            // Smaller priority value = higher priority. If sticky_priority is lower priority than highest_priority,
                            // we ignore the sticky hit to let the router try the highest priority provider.
                            if sticky_priority > highest_priority {
                                is_valid_sticky = false;
                            }
                        }
                    }
                }

                if is_valid_sticky {
                    return Ok(RouteDecision {
                        provider_name: provider,
                        selection_reason: RouteSelectionReason::Sticky,
                        sticky_hit: true,
                    });
                } else {
                    state.sticky_sessions.remove(session_key);
                }
            }
        }

        let untried_providers: Vec<String> = all_providers
            .iter()
            .filter(|provider| !is_excluded(excluded_providers, provider))
            .cloned()
            .collect();

        let healthy_untried: Vec<String> = untried_providers
            .iter()
            .filter(|provider| self.is_provider_healthy(provider))
            .cloned()
            .collect();

        if !healthy_untried.is_empty() {
            return self
                .select_provider_by_strategy(model, &healthy_untried, snapshot)
                .map(|provider_name| RouteDecision {
                    provider_name,
                    selection_reason: RouteSelectionReason::Healthy,
                    sticky_hit: false,
                });
        }

        if !untried_providers.is_empty() {
            return Ok(RouteDecision {
                provider_name: self.select_degraded_provider(&untried_providers, snapshot),
                selection_reason: RouteSelectionReason::Degraded,
                sticky_hit: false,
            });
        }

        let healthy_providers: Vec<String> = all_providers
            .iter()
            .filter(|provider| self.is_provider_healthy(provider))
            .cloned()
            .collect();

        if !healthy_providers.is_empty() {
            return self
                .select_provider_by_strategy(model, &healthy_providers, snapshot)
                .map(|provider_name| RouteDecision {
                    provider_name,
                    selection_reason: RouteSelectionReason::Healthy,
                    sticky_hit: false,
                });
        }

        if !all_providers.is_empty() {
            return Ok(RouteDecision {
                provider_name: self.select_degraded_provider(&all_providers, snapshot),
                selection_reason: RouteSelectionReason::Degraded,
                sticky_hit: false,
            });
        }

        Err(AppError::NoProviderAvailable(model.to_string()))
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

    fn priority_weighted_least_conn(
        &self,
        providers: &[String],
        snapshot: &ConfigSnapshot,
    ) -> String {
        let mut candidates = providers.to_vec();
        candidates.sort_by(|a, b| {
            // 1. Priority (ascending, i.e., smaller priority value is higher priority)
            let a_priority = snapshot
                .find_provider(a)
                .map(|p| p.priority)
                .unwrap_or(i64::MAX);
            let b_priority = snapshot
                .find_provider(b)
                .map(|p| p.priority)
                .unwrap_or(i64::MAX);
            let prio_cmp = a_priority.cmp(&b_priority);
            if prio_cmp != std::cmp::Ordering::Equal {
                return prio_cmp;
            }

            // 2. Score = connections / weight (if weights differ, to load balance)
            // If weights are equal, this naturally compares connection counts.
            let a_weight = snapshot.find_provider(a).map(|p| p.weight).unwrap_or(10) as f64;
            let b_weight = snapshot.find_provider(b).map(|p| p.weight).unwrap_or(10) as f64;

            let a_conns = snapshot.registry.connection_count(a) as f64;
            let b_conns = snapshot.registry.connection_count(b) as f64;

            let a_eff_weight = if a_weight > 0.0 { a_weight } else { 1.0 };
            let b_eff_weight = if b_weight > 0.0 { b_weight } else { 1.0 };

            let a_score = a_conns / a_eff_weight;
            let b_score = b_conns / b_eff_weight;

            let score_cmp = a_score
                .partial_cmp(&b_score)
                .unwrap_or(std::cmp::Ordering::Equal);
            if score_cmp != std::cmp::Ordering::Equal {
                return score_cmp;
            }

            // 3. Name (lexicographical)
            a.cmp(b)
        });

        candidates[0].clone()
    }

    fn select_provider_by_strategy(
        &self,
        model: &str,
        providers: &[String],
        snapshot: &ConfigSnapshot,
    ) -> Result<String, AppError> {
        let strategy: Strategy = snapshot
            .config
            .routing
            .strategy
            .parse()
            .map_err(|_| AppError::ConfigError("Invalid routing strategy".to_string()))?;
        Ok(match strategy {
            Strategy::RoundRobin => self.round_robin(model, providers),
            Strategy::WeightedRoundRobin => {
                self.weighted_round_robin(model, providers, &snapshot.provider_weights)
            }
            Strategy::LeastConnections => self.least_connections(providers, snapshot),
            Strategy::PriorityWeightedLeastConn => {
                self.priority_weighted_least_conn(providers, snapshot)
            }
        })
    }

    fn select_degraded_provider(&self, providers: &[String], snapshot: &ConfigSnapshot) -> String {
        let mut candidates = providers.to_vec();
        candidates.sort_by(|left, right| {
            let left_priority = snapshot
                .find_provider(left)
                .map(|provider| provider.priority)
                .unwrap_or(i64::MAX);
            let right_priority = snapshot
                .find_provider(right)
                .map(|provider| provider.priority)
                .unwrap_or(i64::MAX);

            left_priority
                .cmp(&right_priority)
                .then_with(|| {
                    self.provider_failure_rank(left)
                        .cmp(&self.provider_failure_rank(right))
                })
                .then_with(|| left.cmp(right))
        });
        candidates[0].clone()
    }

    fn provider_failure_rank(&self, provider: &str) -> (usize, Option<std::time::Instant>) {
        self.health
            .get(provider)
            .map(|entry| {
                (
                    entry.consecutive_failures.load(Ordering::Relaxed),
                    *entry.last_failure.lock(),
                )
            })
            .unwrap_or((usize::MAX, None))
    }

    // --- Provider health tracking ---

    /// Record a provider failure. Marks unhealthy after > 3 consecutive failures.
    ///
    /// Returns true when this failure leaves the provider unhealthy.
    pub fn record_provider_failure(&self, provider: &str) -> bool {
        let entry = self
            .health
            .entry(provider.to_string())
            .or_insert_with(ProviderHealth::new);
        let failures = entry.consecutive_failures.fetch_add(1, Ordering::Relaxed) + 1;
        *entry.last_failure.lock() = Some(std::time::Instant::now());
        if failures > 3 {
            entry.is_healthy.store(false, Ordering::Relaxed);
            return true;
        }
        false
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

    /// Check for recovery: mark providers healthy again if last failure > 10s ago.
    pub fn check_recovery(&self) {
        let now = std::time::Instant::now();
        for entry in self.health.iter() {
            if !entry.is_healthy.load(Ordering::Relaxed) {
                let last = *entry.last_failure.lock();
                if let Some(last_failure_time) = last {
                    if now.duration_since(last_failure_time).as_secs() > 10 {
                        entry.is_healthy.store(true, Ordering::Relaxed);
                        entry.consecutive_failures.store(0, Ordering::Relaxed);
                    }
                }
            }
        }
    }
}

fn is_excluded(excluded_providers: Option<&HashSet<String>>, provider: &str) -> bool {
    excluded_providers
        .map(|providers| providers.contains(provider))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        AdminConfig, AppConfig, AuthConfig, CostConfig, DatabaseConfig, ModelRule, ProviderConfig,
        RetryConfig, RoutingConfig, ServerConfig, StickyConfig, TlsConfig,
    };
    use crate::config_service::{ConfigService, ConfigSnapshot};
    use std::collections::{HashMap, HashSet};
    use std::sync::Arc;

    /// Helper: create a minimal ModelRouter with given providers and weights
    fn make_router(strategy: Strategy, model: &str, providers: &[(&str, u32)]) -> ModelRouter {
        let _ = (strategy, model, providers);
        ModelRouter {
            rr_counters: Arc::new(dashmap::DashMap::new()),
            health: Arc::new(dashmap::DashMap::new()),
        }
    }

    fn test_provider(name: &str) -> ProviderConfig {
        ProviderConfig {
            name: name.to_string(),
            adapter: crate::config::ProviderAdapterKind::Openai,
            protocols: vec![crate::config::ProviderProtocol::Completions],
            base_url: "https://api.example.com".to_string(),
            api_key: "test-secret".to_string(),
            models: vec![ModelRule::enabled("gpt-4o")],
            weight: 10,
            max_connections: 10,
            timeout_secs: 30,
            headers: HashMap::new(),
            api_version: None,
            status: "enabled".to_string(),
            priority: 0,
            group: "default".to_string(),
        }
    }

    fn test_provider_with_priority(name: &str, priority: i64) -> ProviderConfig {
        let mut provider = test_provider(name);
        provider.priority = priority;
        provider
    }

    fn test_config(strategy: &str) -> AppConfig {
        AppConfig {
            server: ServerConfig {
                host: "127.0.0.1".to_string(),
                port: 15000,
                tls: TlsConfig::default(),
            },
            providers: vec![test_provider("provider-a"), test_provider("provider-b")],
            routing: RoutingConfig {
                strategy: strategy.to_string(),
                sticky: StickyConfig::default(),
                default_model: None,
            },
            retry: RetryConfig {
                max_attempts: 1,
                initial_backoff_ms: 1,
                max_backoff_ms: 1,
                retryable_statuses: vec![429],
            },
            cost: CostConfig {
                currency: "USD".to_string(),
                default_input_per_1k: 0.0,
                default_output_per_1k: 0.0,
                models: HashMap::new(),
            },
            admin: AdminConfig {
                token: "admin-token".to_string(),
            },
            auth: AuthConfig {
                enabled: false,
                keys: Vec::new(),
            },
            database: DatabaseConfig {
                path: ":memory:".to_string(),
            },
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
        for _ in 0..3 {
            assert!(!router.record_provider_failure("unhealthy"));
        }
        assert!(router.record_provider_failure("unhealthy"));
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
            assert!(!router.record_provider_failure("provider"));
        }
        assert!(router.is_provider_healthy("provider"));

        // 4th failure triggers unhealthy
        assert!(router.record_provider_failure("provider"));
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
            "least_connections".parse::<Strategy>().unwrap(),
            Strategy::LeastConnections
        );
        assert!("sticky_session".parse::<Strategy>().is_err());
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

    #[tokio::test]
    async fn route_uses_strategy_from_current_snapshot() {
        let config_service =
            Arc::new(ConfigService::from_config(test_config("round_robin")).unwrap());
        let state = Arc::new(
            crate::server::AppState::new(config_service.clone())
                .await
                .unwrap(),
        );
        let round_robin_snapshot = state.config_service.snapshot();

        let first = state
            .router
            .route(
                "gpt-4o",
                &Operation::ChatCompletions,
                &state,
                &round_robin_snapshot,
                "",
            )
            .unwrap();
        assert_eq!(first, "provider-a");

        let mut weighted_config = test_config("weighted_round_robin");
        weighted_config.providers[0].weight = 10;
        weighted_config.providers[1].weight = 1;
        let weighted_snapshot = ConfigSnapshot::build(weighted_config).unwrap();

        let next = state
            .router
            .route(
                "gpt-4o",
                &Operation::ChatCompletions,
                &state,
                &weighted_snapshot,
                "",
            )
            .unwrap();
        assert_eq!(next, "provider-a");
    }

    #[tokio::test]
    async fn degraded_fallback_prefers_lower_priority_provider() {
        let config = AppConfig {
            server: ServerConfig {
                host: "127.0.0.1".to_string(),
                port: 15000,
                tls: TlsConfig::default(),
            },
            providers: vec![
                test_provider_with_priority("provider-a", 10),
                test_provider_with_priority("provider-b", 1),
            ],
            routing: RoutingConfig {
                strategy: "round_robin".to_string(),
                sticky: StickyConfig::default(),
                default_model: None,
            },
            retry: RetryConfig {
                max_attempts: 1,
                initial_backoff_ms: 1,
                max_backoff_ms: 1,
                retryable_statuses: vec![429],
            },
            cost: CostConfig {
                currency: "USD".to_string(),
                default_input_per_1k: 0.0,
                default_output_per_1k: 0.0,
                models: HashMap::new(),
            },
            admin: AdminConfig {
                token: "admin-token".to_string(),
            },
            auth: AuthConfig {
                enabled: false,
                keys: Vec::new(),
            },
            database: DatabaseConfig {
                path: ":memory:".to_string(),
            },
        };

        let config_service = Arc::new(ConfigService::from_config(config).unwrap());
        let state = Arc::new(
            crate::server::AppState::new(config_service.clone())
                .await
                .unwrap(),
        );
        let snapshot = state.config_service.snapshot();

        for _ in 0..4 {
            state.router.record_provider_failure("provider-a");
            state.router.record_provider_failure("provider-b");
        }

        let picked = state
            .router
            .route("gpt-4o", &Operation::ChatCompletions, &state, &snapshot, "")
            .unwrap();
        assert_eq!(picked, "provider-b");
    }

    #[tokio::test]
    async fn route_with_exclusions_skips_already_tried_provider() {
        let config_service =
            Arc::new(ConfigService::from_config(test_config("round_robin")).unwrap());
        let state = Arc::new(
            crate::server::AppState::new(config_service.clone())
                .await
                .unwrap(),
        );
        let snapshot = state.config_service.snapshot();

        let first = state
            .router
            .route("gpt-4o", &Operation::ChatCompletions, &state, &snapshot, "")
            .unwrap();
        let excluded = HashSet::from([first.clone()]);
        let second = state
            .router
            .route_decision_with_exclusions(
                "gpt-4o",
                &Operation::ChatCompletions,
                &state,
                &snapshot,
                "",
                Some(&excluded),
            )
            .unwrap()
            .provider_name;

        assert_ne!(first, second);
    }

    #[tokio::test]
    async fn test_priority_weighted_least_conn() {
        let mut config = test_config("priority_weighted_least_conn");
        config.providers = vec![
            ProviderConfig {
                priority: 1,
                weight: 10,
                name: "provider-c-low-prio".to_string(),
                ..test_provider("provider-c-low-prio")
            },
            ProviderConfig {
                priority: 0,
                weight: 20,
                name: "provider-a-high-weight".to_string(),
                ..test_provider("provider-a-high-weight")
            },
            ProviderConfig {
                priority: 0,
                weight: 10,
                name: "provider-b-low-weight".to_string(),
                ..test_provider("provider-b-low-weight")
            },
        ];

        let config_service = Arc::new(ConfigService::from_config(config).unwrap());
        let state = Arc::new(
            crate::server::AppState::new(config_service.clone())
                .await
                .unwrap(),
        );
        let snapshot = state.config_service.snapshot();

        let picked = state
            .router
            .route("gpt-4o", &Operation::ChatCompletions, &state, &snapshot, "")
            .unwrap();
        assert_eq!(picked, "provider-a-high-weight");

        snapshot
            .registry
            .record_connection("provider-a-high-weight");
        snapshot
            .registry
            .record_connection("provider-a-high-weight");
        snapshot.registry.record_connection("provider-b-low-weight");

        let picked2 = state
            .router
            .route("gpt-4o", &Operation::ChatCompletions, &state, &snapshot, "")
            .unwrap();
        assert_eq!(picked2, "provider-a-high-weight");

        snapshot.registry.record_connection("provider-b-low-weight");
        let picked3 = state
            .router
            .route("gpt-4o", &Operation::ChatCompletions, &state, &snapshot, "")
            .unwrap();
        assert_eq!(picked3, "provider-a-high-weight");

        snapshot
            .registry
            .record_connection("provider-a-high-weight");
        snapshot
            .registry
            .record_connection("provider-a-high-weight");
        snapshot
            .registry
            .record_connection("provider-a-high-weight");
        let picked4 = state
            .router
            .route("gpt-4o", &Operation::ChatCompletions, &state, &snapshot, "")
            .unwrap();
        assert_eq!(picked4, "provider-b-low-weight");
    }

    #[tokio::test]
    async fn test_sticky_session_priority_fallback_retry() {
        let mut config = test_config("priority_weighted_least_conn");
        config.routing.sticky.enabled = true;
        config.routing.sticky.ttl_secs = 300;
        config.routing.sticky.fallback_retry_interval_secs = Some(0);

        config.providers = vec![
            ProviderConfig {
                priority: 0,
                name: "provider-high".to_string(),
                ..test_provider("provider-high")
            },
            ProviderConfig {
                priority: 1,
                name: "provider-low".to_string(),
                ..test_provider("provider-low")
            },
        ];

        let config_service = Arc::new(ConfigService::from_config(config).unwrap());
        let state = Arc::new(
            crate::server::AppState::new(config_service.clone())
                .await
                .unwrap(),
        );
        let snapshot = state.config_service.snapshot();

        for _ in 0..4 {
            state.router.record_provider_failure("provider-high");
        }
        assert!(!state.router.is_provider_healthy("provider-high"));

        let picked = state
            .router
            .route(
                "gpt-4o",
                &Operation::ChatCompletions,
                &state,
                &snapshot,
                "session-123",
            )
            .unwrap();
        assert_eq!(picked, "provider-low");

        state.sticky_sessions.set_with_ttl(
            "session-123".to_string(),
            "provider-low".to_string(),
            std::time::Duration::from_secs(60),
        );

        state.router.record_provider_success("provider-high");
        assert!(state.router.is_provider_healthy("provider-high"));

        let picked2 = state
            .router
            .route(
                "gpt-4o",
                &Operation::ChatCompletions,
                &state,
                &snapshot,
                "session-123",
            )
            .unwrap();
        assert_eq!(picked2, "provider-high");
    }
}
