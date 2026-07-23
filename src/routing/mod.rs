pub mod sticky;

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use crate::config::{AppConfig, ProviderProtocol};
use crate::config_service::{ConfigSnapshot, ModelEndpoint};
use crate::error::AppError;
use crate::protocol::common::Operation;
use crate::server::AppState;

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
    pub endpoint: ModelEndpoint,
    pub selection_reason: RouteSelectionReason,
    pub sticky_hit: bool,
}

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

pub struct ModelRouter {
    rr_counters: Arc<dashmap::DashMap<String, AtomicUsize>>,
    health: Arc<dashmap::DashMap<String, ProviderHealth>>,
}

impl ModelRouter {
    pub fn new(config: &AppConfig) -> anyhow::Result<Self> {
        let health = Arc::new(dashmap::DashMap::new());
        for provider in &config.providers {
            if provider.is_enabled() && provider.has_enabled_endpoint() {
                health.insert(provider.name.clone(), ProviderHealth::new());
            }
        }
        Ok(Self {
            rr_counters: Arc::new(dashmap::DashMap::new()),
            health,
        })
    }

    pub fn route(
        &self,
        model: &str,
        operation: &crate::protocol::common::Operation,
        state: &AppState,
        snapshot: &ConfigSnapshot,
        session_key: &str,
        key: Option<&crate::config::AuthKey>,
    ) -> Result<ModelEndpoint, AppError> {
        self.route_decision_with_exclusions(
            model,
            operation,
            state,
            snapshot,
            session_key,
            key,
            None,
        )
        .map(|decision| decision.endpoint)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn route_decision_with_exclusions(
        &self,
        model: &str,
        operation: &Operation,
        state: &AppState,
        snapshot: &ConfigSnapshot,
        session_key: &str,
        key: Option<&crate::config::AuthKey>,
        excluded_providers: Option<&HashSet<String>>,
    ) -> Result<RouteDecision, AppError> {
        self.route_decision_for_protocols_with_exclusions(
            model,
            operation,
            &[operation.provider_protocol()],
            state,
            snapshot,
            session_key,
            key,
            excluded_providers,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn route_decision_for_protocols_with_exclusions(
        &self,
        model: &str,
        operation: &Operation,
        candidate_protocols: &[ProviderProtocol],
        state: &AppState,
        snapshot: &ConfigSnapshot,
        session_key: &str,
        key: Option<&crate::config::AuthKey>,
        excluded_providers: Option<&HashSet<String>>,
    ) -> Result<RouteDecision, AppError> {
        let candidate_protocols = if candidate_protocols.is_empty() {
            vec![operation.provider_protocol()]
        } else {
            candidate_protocols.to_vec()
        };
        let all_endpoints: Vec<ModelEndpoint> = snapshot
            .endpoints_for_alias(model, key)
            .into_iter()
            .filter(|ep| candidate_protocols.contains(&ep.protocol))
            .collect();

        if all_endpoints.is_empty() {
            return Err(AppError::ModelNotFound(model.to_string()));
        }

        let all_provider_names: Vec<String> = all_endpoints
            .iter()
            .map(|ep| ep.provider_name.clone())
            .collect();
        self.ensure_provider_health_entries(&all_provider_names);
        self.check_recovery();

        for candidate_protocol in &candidate_protocols {
            let protocol_endpoints: Vec<&ModelEndpoint> = all_endpoints
                .iter()
                .filter(|ep| ep.protocol == *candidate_protocol)
                .collect();
            if protocol_endpoints.is_empty() {
                continue;
            }

            let sticky_config = &snapshot.config.routing.sticky;
            if sticky_config.enabled {
                let ttl = std::time::Duration::from_secs(sticky_config.ttl_secs);
                if let Some((provider, pinned_at)) =
                    state.sticky_sessions.get_with_pinned_at(session_key, ttl)
                {
                    let sticky_excluded = is_excluded(excluded_providers, &provider);
                    let sticky_endpoint = protocol_endpoints
                        .iter()
                        .find(|ep| ep.provider_name == provider)
                        .copied();
                    let sticky_healthy = self.is_provider_healthy(&provider);

                    let mut is_valid_sticky =
                        !sticky_excluded && sticky_endpoint.is_some() && sticky_healthy;

                    if is_valid_sticky {
                        let fallback_retry_interval =
                            sticky_config.fallback_retry_interval_secs.unwrap_or(60);
                        if pinned_at.elapsed().as_secs() >= fallback_retry_interval {
                            let best_priority = protocol_endpoints
                                .iter()
                                .filter(|ep| self.is_provider_healthy(&ep.provider_name))
                                .map(|ep| ep.priority)
                                .min();

                            if let Some(best) = best_priority {
                                let sticky_priority =
                                    sticky_endpoint.map(|ep| ep.priority).unwrap_or(i64::MAX);
                                if sticky_priority > best {
                                    is_valid_sticky = false;
                                }
                            }
                        }
                    }

                    if is_valid_sticky {
                        return Ok(RouteDecision {
                            endpoint: sticky_endpoint.unwrap().clone(),
                            selection_reason: RouteSelectionReason::Sticky,
                            sticky_hit: true,
                        });
                    }

                    if all_endpoints.iter().any(|ep| ep.provider_name == provider) {
                        state.sticky_sessions.remove(session_key);
                    }
                }
            }

            let mut priority_groups: Vec<(i64, Vec<&ModelEndpoint>)> = Vec::new();
            {
                let mut groups: HashMap<i64, Vec<&ModelEndpoint>> = HashMap::new();
                for ep in protocol_endpoints {
                    groups.entry(ep.priority).or_default().push(ep);
                }
                let mut keys: Vec<i64> = groups.keys().copied().collect();
                keys.sort_unstable();
                for k in keys {
                    priority_groups.push((k, groups.remove(&k).unwrap()));
                }
            }

            for (_priority, group) in &priority_groups {
                let untried: Vec<&ModelEndpoint> = group
                    .iter()
                    .filter(|ep| !is_excluded(excluded_providers, &ep.provider_name))
                    .copied()
                    .collect();
                if untried.is_empty() {
                    continue;
                }

                let healthy: Vec<&ModelEndpoint> = untried
                    .iter()
                    .filter(|ep| self.is_provider_healthy(&ep.provider_name))
                    .copied()
                    .collect();

                if !healthy.is_empty() {
                    let chosen = self.round_robin(model, &healthy);
                    return Ok(RouteDecision {
                        endpoint: (*chosen).clone(),
                        selection_reason: RouteSelectionReason::Healthy,
                        sticky_hit: false,
                    });
                }
            }
        }

        for candidate_protocol in &candidate_protocols {
            let healthy_all: Vec<&ModelEndpoint> = all_endpoints
                .iter()
                .filter(|ep| ep.protocol == *candidate_protocol)
                .filter(|ep| !is_excluded(excluded_providers, &ep.provider_name))
                .filter(|ep| self.is_provider_healthy(&ep.provider_name))
                .collect();
            if !healthy_all.is_empty() {
                let chosen = self.round_robin(model, &healthy_all);
                return Ok(RouteDecision {
                    endpoint: (*chosen).clone(),
                    selection_reason: RouteSelectionReason::Healthy,
                    sticky_hit: false,
                });
            }
        }

        for candidate_protocol in &candidate_protocols {
            let all_refs: Vec<&ModelEndpoint> = all_endpoints
                .iter()
                .filter(|ep| ep.protocol == *candidate_protocol)
                .filter(|ep| !is_excluded(excluded_providers, &ep.provider_name))
                .collect();
            if !all_refs.is_empty() {
                let degraded = self.select_degraded_endpoint(&all_refs);
                return Ok(RouteDecision {
                    endpoint: (*degraded).clone(),
                    selection_reason: RouteSelectionReason::Degraded,
                    sticky_hit: false,
                });
            }
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

    fn round_robin<'a>(&self, model: &str, endpoints: &[&'a ModelEndpoint]) -> &'a ModelEndpoint {
        let counter = self
            .rr_counters
            .entry(model.to_string())
            .or_insert_with(|| AtomicUsize::new(0));
        let idx = counter.fetch_add(1, Ordering::Relaxed) % endpoints.len();
        endpoints[idx]
    }

    fn select_degraded_endpoint<'a>(
        &self,
        endpoints: &'a [&'a ModelEndpoint],
    ) -> &'a ModelEndpoint {
        endpoints
            .iter()
            .min_by(|a, b| {
                let a_failures = self.provider_failure_count(&a.provider_name);
                let b_failures = self.provider_failure_count(&b.provider_name);
                a_failures
                    .cmp(&b_failures)
                    .then_with(|| a.priority.cmp(&b.priority))
                    .then_with(|| a.provider_name.cmp(&b.provider_name))
            })
            .copied()
            .unwrap_or(endpoints[0])
    }

    fn provider_failure_count(&self, provider: &str) -> usize {
        self.health
            .get(provider)
            .map(|entry| entry.consecutive_failures.load(Ordering::Relaxed))
            .unwrap_or(usize::MAX)
    }

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

    pub fn record_provider_success(&self, provider: &str) {
        let entry = self
            .health
            .entry(provider.to_string())
            .or_insert_with(ProviderHealth::new);
        entry.consecutive_failures.store(0, Ordering::Relaxed);
        entry.is_healthy.store(true, Ordering::Relaxed);
    }

    pub fn is_provider_healthy(&self, provider: &str) -> bool {
        self.health
            .get(provider)
            .map(|h| h.is_healthy.load(Ordering::Relaxed))
            .unwrap_or(false)
    }

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
        AppConfig, AuthKey, CostConfig, EndpointConfig, ModelRule, ProviderConfig,
        ProviderProtocol, RetryConfig, RoutingConfig, StickyConfig, UpstreamConfig,
    };
    use crate::config_service::{ConfigService, ConfigSnapshot, ModelEndpoint};
    use crate::protocol::common::Operation;
    use crate::server::RuntimeConfig;
    use std::collections::{HashMap, HashSet};
    use std::sync::Arc;

    fn make_router() -> ModelRouter {
        ModelRouter {
            rr_counters: Arc::new(dashmap::DashMap::new()),
            health: Arc::new(dashmap::DashMap::new()),
        }
    }

    fn test_provider(name: &str) -> ProviderConfig {
        ProviderConfig {
            name: name.to_string(),
            api_key: "test-secret".to_string(),
            models: vec![ModelRule::enabled("gpt-4o")],
            endpoints: vec![EndpointConfig {
                protocol: ProviderProtocol::Completions,
                base_url: "https://api.example.com/v1/chat/completions".to_string(),
            }],
            headers: HashMap::new(),
            status: "enabled".to_string(),
            priority: 0,
        }
    }

    fn test_provider_with_priority(name: &str, priority: i64) -> ProviderConfig {
        let mut provider = test_provider(name);
        provider.priority = priority;
        provider
    }

    fn test_config() -> AppConfig {
        AppConfig {
            providers: vec![test_provider("provider-a"), test_provider("provider-b")],
            upstream: UpstreamConfig { timeout_secs: 30 },
            routing: RoutingConfig {
                sticky: StickyConfig::default(),
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
            keys: Vec::new(),
        }
    }

    #[test]
    fn test_round_robin_cycles() {
        let router = make_router();
        let ep_a = ModelEndpoint {
            alias_name: "gpt-4o".into(),
            name: "gpt-4o".into(),
            provider_name: "a".into(),
            priority: 0,
            protocol: ProviderProtocol::Completions,
            base_url: "https://a.example.com/v1/chat/completions".into(),
            adapter_kind: "openai",
        };
        let ep_b = ep_a.clone();
        let ep_b = ModelEndpoint {
            provider_name: "b".into(),
            ..ep_b
        };
        let ep_c = ep_a.clone();
        let ep_c = ModelEndpoint {
            provider_name: "c".into(),
            ..ep_c
        };
        let endpoints: Vec<&ModelEndpoint> = vec![&ep_a, &ep_b, &ep_c];

        let results: Vec<String> = (0..6)
            .map(|_| {
                router
                    .round_robin("gpt-4o", &endpoints)
                    .provider_name
                    .clone()
            })
            .collect();
        assert_eq!(results, vec!["a", "b", "c", "a", "b", "c"]);
    }

    #[test]
    fn test_provider_health_tracking() {
        let router = make_router();
        router.ensure_provider_health_entries(&["healthy".to_string(), "unhealthy".to_string()]);
        assert!(router.is_provider_healthy("healthy"));
        assert!(router.is_provider_healthy("unhealthy"));
        assert!(!router.is_provider_healthy("unknown-provider"));

        for _ in 0..3 {
            assert!(!router.record_provider_failure("unhealthy"));
        }
        assert!(router.record_provider_failure("unhealthy"));
        assert!(!router.is_provider_healthy("unhealthy"));
        assert!(router.is_provider_healthy("healthy"));

        router.record_provider_success("unhealthy");
        assert!(router.is_provider_healthy("unhealthy"));
    }

    #[test]
    fn test_provider_health_not_triggered_below_threshold() {
        let router = make_router();
        router.ensure_provider_health_entry("provider");
        for _ in 0..3 {
            assert!(!router.record_provider_failure("provider"));
        }
        assert!(router.is_provider_healthy("provider"));
        assert!(router.record_provider_failure("provider"));
        assert!(!router.is_provider_healthy("provider"));
    }

    #[tokio::test]
    async fn route_uses_round_robin_for_equal_priority_providers() {
        let config_service = Arc::new(ConfigService::from_config(test_config()).unwrap());
        let state = Arc::new(
            crate::server::AppState::new(
                config_service.clone(),
                RuntimeConfig::in_memory("admin-token"),
            )
            .await
            .unwrap(),
        );
        let snapshot = state.config_service.snapshot();

        let first = state
            .router
            .route(
                "gpt-4o",
                &Operation::Completions,
                &state,
                &snapshot,
                "",
                None,
            )
            .unwrap();
        assert_eq!(first.provider_name, "provider-a");
    }

    #[tokio::test]
    async fn degraded_fallback_prefers_lower_priority_provider() {
        let config = AppConfig {
            providers: vec![
                test_provider_with_priority("provider-a", 10),
                test_provider_with_priority("provider-b", 1),
            ],
            upstream: UpstreamConfig { timeout_secs: 30 },
            routing: RoutingConfig {
                sticky: StickyConfig::default(),
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
            keys: Vec::new(),
        };

        let config_service = Arc::new(ConfigService::from_config(config).unwrap());
        let state = Arc::new(
            crate::server::AppState::new(
                config_service.clone(),
                RuntimeConfig::in_memory("admin-token"),
            )
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
            .route(
                "gpt-4o",
                &Operation::Completions,
                &state,
                &snapshot,
                "",
                None,
            )
            .unwrap();
        assert_eq!(picked.provider_name, "provider-b");
    }

    #[tokio::test]
    async fn priority_tier_falls_through_when_higher_priority_unhealthy() {
        let config = AppConfig {
            providers: vec![
                test_provider_with_priority("provider-high", 0),
                test_provider_with_priority("provider-low", 10),
            ],
            upstream: UpstreamConfig { timeout_secs: 30 },
            routing: RoutingConfig {
                sticky: StickyConfig::default(),
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
            keys: Vec::new(),
        };

        let config_service = Arc::new(ConfigService::from_config(config).unwrap());
        let state = Arc::new(
            crate::server::AppState::new(
                config_service.clone(),
                RuntimeConfig::in_memory("admin-token"),
            )
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
                &Operation::Completions,
                &state,
                &snapshot,
                "",
                None,
            )
            .unwrap();
        assert_eq!(picked.provider_name, "provider-low");
    }

    #[tokio::test]
    async fn route_with_exclusions_skips_already_tried_provider() {
        let config_service = Arc::new(ConfigService::from_config(test_config()).unwrap());
        let state = Arc::new(
            crate::server::AppState::new(
                config_service.clone(),
                RuntimeConfig::in_memory("admin-token"),
            )
            .await
            .unwrap(),
        );
        let snapshot = state.config_service.snapshot();

        let first = state
            .router
            .route_decision_with_exclusions(
                "gpt-4o",
                &Operation::Completions,
                &state,
                &snapshot,
                "",
                None,
                None,
            )
            .unwrap();
        let excluded = HashSet::from([first.endpoint.provider_name.clone()]);
        let second = state
            .router
            .route_decision_with_exclusions(
                "gpt-4o",
                &Operation::Completions,
                &state,
                &snapshot,
                "",
                None,
                Some(&excluded),
            )
            .unwrap()
            .endpoint;

        assert_ne!(first.endpoint.provider_name, second.provider_name);
    }

    #[tokio::test]
    async fn route_with_exclusions_does_not_fallback_to_excluded_provider() {
        let mut config = test_config();
        config.providers = vec![test_provider("provider-a")];
        let config_service = Arc::new(ConfigService::from_config(config).unwrap());
        let state = Arc::new(
            crate::server::AppState::new(
                config_service.clone(),
                RuntimeConfig::in_memory("admin-token"),
            )
            .await
            .unwrap(),
        );
        let snapshot = state.config_service.snapshot();
        let excluded = HashSet::from(["provider-a".to_string()]);

        let err = state
            .router
            .route_decision_with_exclusions(
                "gpt-4o",
                &Operation::Completions,
                &state,
                &snapshot,
                "",
                None,
                Some(&excluded),
            )
            .unwrap_err();

        assert!(matches!(err, AppError::NoProviderAvailable(_)));
    }

    #[tokio::test]
    async fn key_alias_produces_key_scoped_endpoint() {
        let mut config = test_config();
        config.keys.push(AuthKey {
            id: "key-1".into(),
            name: None,
            key: "secret-1".into(),
            models: vec![],
            model_aliases: HashMap::from([("fast".into(), "gpt-4o".into())]),
            allowed_providers: Vec::new(),
            status: "enabled".into(),
            labels: None,
        });

        let snapshot = ConfigSnapshot::build(config).unwrap();
        let key = snapshot.auth_key_for_secret("secret-1").unwrap();

        let with_key = snapshot.endpoints_for_alias("fast", Some(&key));
        assert!(!with_key.is_empty());
        assert_eq!(with_key[0].name, "gpt-4o");
        assert_eq!(with_key[0].alias_name, "fast");

        let without_key = snapshot.endpoints_for_alias("fast", None);
        assert!(without_key.is_empty());
    }

    #[tokio::test]
    async fn provider_level_connection_count_sums_all_endpoint_adapters() {
        let mut config = test_config();
        config.providers = vec![ProviderConfig {
            name: "provider-a".to_string(),
            api_key: "test-secret".to_string(),
            models: vec![ModelRule::enabled("gpt-4o")],
            endpoints: vec![
                EndpointConfig {
                    protocol: ProviderProtocol::Completions,
                    base_url: "https://api.example.com/v1/chat/completions".to_string(),
                },
                EndpointConfig {
                    protocol: ProviderProtocol::Responses,
                    base_url: "https://api.example.com/v1/responses".to_string(),
                },
            ],
            headers: HashMap::new(),
            status: "enabled".to_string(),
            priority: 0,
        }];

        let snapshot = ConfigSnapshot::build(config).unwrap();
        snapshot
            .registry
            .record_connection("provider-a", ProviderProtocol::Completions);
        snapshot
            .registry
            .record_connection("provider-a", ProviderProtocol::Responses);

        assert_eq!(snapshot.registry.connection_count("provider-a"), 2);
    }

    #[tokio::test]
    async fn key_allowed_providers_filters_routing_candidates() {
        let mut config = test_config();
        config.providers[0].priority = 10;
        config.providers[1].priority = 10;
        config.keys.push(AuthKey {
            id: "key-1".into(),
            name: None,
            key: "secret-1".into(),
            models: vec![],
            model_aliases: HashMap::new(),
            allowed_providers: vec!["provider-b".into()],
            status: "enabled".into(),
            labels: None,
        });

        let config_service = Arc::new(ConfigService::from_config(config).unwrap());
        let state = Arc::new(
            crate::server::AppState::new(
                config_service.clone(),
                RuntimeConfig::in_memory("admin-token"),
            )
            .await
            .unwrap(),
        );
        let snapshot = state.config_service.snapshot();
        let key = snapshot.auth_key_for_secret("secret-1").unwrap();

        let picked = state
            .router
            .route(
                "gpt-4o",
                &Operation::Completions,
                &state,
                &snapshot,
                "",
                Some(&key),
            )
            .unwrap();
        assert_eq!(picked.provider_name, "provider-b");
    }

    #[tokio::test]
    async fn sticky_does_not_cross_protocol_priority_tiers() {
        let mut fallback = test_provider("fallback-completions");
        fallback.priority = 0;
        fallback.endpoints = vec![EndpointConfig {
            protocol: ProviderProtocol::Completions,
            base_url: "https://fallback.example.com/v1/chat/completions".to_string(),
        }];

        let mut native = test_provider("native-responses");
        native.priority = 100;
        native.endpoints = vec![EndpointConfig {
            protocol: ProviderProtocol::Responses,
            base_url: "https://native.example.com/v1/responses".to_string(),
        }];

        let mut config = test_config();
        config.providers = vec![fallback, native];
        config.routing.sticky.enabled = true;
        config.routing.sticky.ttl_secs = 60;

        let config_service = Arc::new(ConfigService::from_config(config).unwrap());
        let state = Arc::new(
            crate::server::AppState::new(
                config_service.clone(),
                RuntimeConfig::in_memory("admin-token"),
            )
            .await
            .unwrap(),
        );
        let snapshot = state.config_service.snapshot();
        state
            .sticky_sessions
            .set("session-a".to_string(), "fallback-completions".to_string());

        let picked = state
            .router
            .route_decision_for_protocols_with_exclusions(
                "gpt-4o",
                &Operation::Responses,
                &[ProviderProtocol::Responses, ProviderProtocol::Completions],
                &state,
                &snapshot,
                "session-a",
                None,
                None,
            )
            .unwrap();

        assert_eq!(picked.endpoint.provider_name, "native-responses");
        assert_eq!(picked.endpoint.protocol, ProviderProtocol::Responses);
        assert!(!picked.sticky_hit);
    }
}
