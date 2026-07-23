use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use arc_swap::ArcSwap;
use parking_lot::Mutex;
use serde::Serialize;

use crate::config::{
    expand_tilde, wildcard_match, AppConfig, AuthKey, EndpointConfig, ModelPricing, ModelRule,
    ProviderConfig, ProviderProtocol,
};
use crate::provider::ProviderRegistry;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelEndpoint {
    pub alias_name: String,
    pub name: String,
    pub provider_name: String,
    pub priority: i64,
    pub protocol: ProviderProtocol,
    pub base_url: String,
    pub adapter_kind: &'static str,
}

#[derive(Clone)]
pub struct ConfigService {
    path: PathBuf,
    path_backed: bool,
    snapshot: Arc<ArcSwap<ConfigSnapshot>>,
    write_lock: Arc<Mutex<()>>,
    file_cache: Arc<Mutex<ConfigFileCache>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FileFingerprint {
    len: u64,
    modified: Option<SystemTime>,
}

#[derive(Debug, Clone)]
struct ConfigFileCache {
    raw_yaml: String,
    fingerprint: Option<FileFingerprint>,
}

pub struct ConfigSnapshot {
    pub raw_config: AppConfig,
    pub config: AppConfig,
    pub registry: ProviderRegistry,
    pub model_endpoints: Vec<ModelEndpoint>,
    pub key_endpoints: HashMap<String, Vec<ModelEndpoint>>,
    api_keys_by_secret: HashMap<String, AuthKey>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderView {
    pub name: String,
    pub api_key: String,
    pub models: Vec<ModelRule>,
    pub endpoints: Vec<EndpointConfig>,
    pub headers: HashMap<String, String>,
    pub status: String,
    pub priority: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct AuthKeyView {
    pub id: String,
    pub name: Option<String>,
    pub key: String,
    pub status: String,
    pub labels: Option<String>,
    pub allowed_models: Vec<ModelRule>,
    pub model_aliases: HashMap<String, String>,
    pub allowed_providers: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AliasView {
    pub alias: String,
    pub target_model: String,
    pub provider_name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelCatalogEntry {
    pub name: String,
    pub kind: String,
    pub provider_name: Option<String>,
    pub protocols: Vec<ProviderProtocol>,
    #[serde(skip_serializing)]
    pub target_model: Option<String>,
    #[serde(skip_serializing)]
    pub model_name: String,
    #[serde(skip_serializing)]
    pub aliases: Vec<String>,
    #[serde(skip_serializing)]
    pub selectable_names: Vec<String>,
}

impl ConfigService {
    pub fn new(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let path = expand_tilde(path.as_ref());
        let raw_yaml = fs::read_to_string(&path)?;
        let raw_config = Self::parse_raw_yaml(&raw_yaml)?;
        let snapshot = ConfigSnapshot::build(raw_config)?;
        let fingerprint = Self::file_fingerprint(&path)?;
        Ok(Self {
            path,
            path_backed: true,
            snapshot: Arc::new(ArcSwap::from_pointee(snapshot)),
            write_lock: Arc::new(Mutex::new(())),
            file_cache: Arc::new(Mutex::new(ConfigFileCache {
                raw_yaml,
                fingerprint: Some(fingerprint),
            })),
        })
    }

    pub fn from_config(config: AppConfig) -> anyhow::Result<Self> {
        let raw_yaml = serde_yaml::to_string(&config)?;
        let snapshot = ConfigSnapshot::build(config)?;
        Ok(Self {
            path: PathBuf::from("config.yaml"),
            path_backed: false,
            snapshot: Arc::new(ArcSwap::from_pointee(snapshot)),
            write_lock: Arc::new(Mutex::new(())),
            file_cache: Arc::new(Mutex::new(ConfigFileCache {
                raw_yaml,
                fingerprint: None,
            })),
        })
    }

    pub fn snapshot(&self) -> Arc<ConfigSnapshot> {
        if let Err(err) = self.refresh_snapshot_if_needed() {
            tracing::warn!(path = %self.path.display(), error = %err, "Failed to refresh config snapshot");
        }
        self.snapshot.load_full()
    }

    pub fn path_display(&self) -> String {
        self.path.display().to_string()
    }

    pub fn read_raw_yaml(&self) -> anyhow::Result<String> {
        self.refresh_snapshot_if_needed()?;
        Ok(self.file_cache.lock().raw_yaml.clone())
    }

    pub fn replace_raw_yaml(&self, content: &str) -> anyhow::Result<Arc<ConfigSnapshot>> {
        let _guard = self.write_lock.lock();
        let raw_config = Self::parse_raw_yaml(content)?;
        let next_snapshot = ConfigSnapshot::build(raw_config)?;
        if self.path_backed {
            let tmp = self.path.with_extension("yaml.tmp");
            fs::write(&tmp, content)?;
            fs::rename(tmp, &self.path)?;
        }
        self.update_file_cache(content.to_string())?;
        let next = Arc::new(next_snapshot);
        self.snapshot.store(next.clone());
        Ok(next)
    }

    pub fn update<F>(&self, f: F) -> anyhow::Result<Arc<ConfigSnapshot>>
    where
        F: FnOnce(&mut AppConfig) -> anyhow::Result<()>,
    {
        let _guard = self.write_lock.lock();
        self.refresh_snapshot_if_needed_locked()?;
        let mut raw_config = self.snapshot.load_full().raw_config.clone();
        f(&mut raw_config)?;
        raw_config.validate()?;
        let next_snapshot = ConfigSnapshot::build(raw_config.clone())?;
        self.write_raw_config(&raw_config)?;
        let next = Arc::new(next_snapshot);
        self.snapshot.store(next.clone());
        Ok(next)
    }

    fn write_raw_config(&self, config: &AppConfig) -> anyhow::Result<()> {
        let content = serde_yaml::to_string(config)?;
        if self.path_backed {
            let tmp = self.path.with_extension("yaml.tmp");
            fs::write(&tmp, &content)?;
            fs::rename(tmp, &self.path)?;
        }
        self.update_file_cache(content)?;
        Ok(())
    }

    fn parse_raw_yaml(content: &str) -> anyhow::Result<AppConfig> {
        let raw_config: AppConfig = serde_yaml::from_str(content)?;
        raw_config.validate()?;
        Ok(raw_config)
    }

    fn file_fingerprint(path: &Path) -> anyhow::Result<FileFingerprint> {
        let metadata = fs::metadata(path)?;
        Ok(FileFingerprint {
            len: metadata.len(),
            modified: metadata.modified().ok(),
        })
    }

    fn refresh_snapshot_if_needed(&self) -> anyhow::Result<()> {
        if !self.path_backed {
            return Ok(());
        }

        let current_fingerprint = Self::file_fingerprint(&self.path)?;
        if self.file_cache.lock().fingerprint.as_ref() == Some(&current_fingerprint) {
            return Ok(());
        }

        let _guard = self.write_lock.lock();
        self.refresh_snapshot_if_needed_locked()
    }

    fn refresh_snapshot_if_needed_locked(&self) -> anyhow::Result<()> {
        if !self.path_backed {
            return Ok(());
        }

        let current_fingerprint = Self::file_fingerprint(&self.path)?;
        if self.file_cache.lock().fingerprint.as_ref() == Some(&current_fingerprint) {
            return Ok(());
        }

        let raw_yaml = fs::read_to_string(&self.path)?;
        let raw_config = Self::parse_raw_yaml(&raw_yaml)?;
        let next_snapshot = Arc::new(ConfigSnapshot::build(raw_config)?);
        self.snapshot.store(next_snapshot);
        *self.file_cache.lock() = ConfigFileCache {
            raw_yaml,
            fingerprint: Some(current_fingerprint),
        };
        Ok(())
    }

    fn update_file_cache(&self, raw_yaml: String) -> anyhow::Result<()> {
        let fingerprint = if self.path_backed {
            Some(Self::file_fingerprint(&self.path)?)
        } else {
            None
        };
        *self.file_cache.lock() = ConfigFileCache {
            raw_yaml,
            fingerprint,
        };
        Ok(())
    }
}

impl ConfigSnapshot {
    pub fn build(raw_config: AppConfig) -> anyhow::Result<Self> {
        let config = raw_config.expanded()?;
        let registry = ProviderRegistry::from_config(&config)?;
        let (model_endpoints, key_endpoints) = Self::build_model_endpoints(&config);
        let api_keys_by_secret = config
            .keys
            .iter()
            .filter(|key| key.status == "enabled")
            .map(|key| (key.key.clone(), key.clone()))
            .collect();

        Ok(Self {
            raw_config,
            config,
            registry,
            model_endpoints,
            key_endpoints,
            api_keys_by_secret,
        })
    }

    fn build_model_endpoints(
        config: &AppConfig,
    ) -> (Vec<ModelEndpoint>, HashMap<String, Vec<ModelEndpoint>>) {
        let mut global: Vec<ModelEndpoint> = Vec::new();

        for provider in &config.providers {
            if !provider.is_enabled() || !provider.has_enabled_endpoint() {
                continue;
            }
            let adapter_kind = provider_adapter_kind(provider);
            for endpoint in &provider.endpoints {
                for model in provider.models.iter().filter(|m| m.is_enabled()) {
                    for public_name in model.public_names() {
                        global.push(ModelEndpoint {
                            alias_name: public_name,
                            name: model.name.clone(),
                            provider_name: provider.name.clone(),
                            priority: provider.priority,
                            protocol: endpoint.protocol,
                            base_url: endpoint.base_url.clone(),
                            adapter_kind,
                        });
                    }
                }
            }
        }

        let mut key_endpoints: HashMap<String, Vec<ModelEndpoint>> = HashMap::new();
        for key in config.keys.iter().filter(|k| k.status == "enabled") {
            if key.model_aliases.is_empty() {
                continue;
            }
            let mut extras = Vec::new();
            for (alias, target) in &key.model_aliases {
                for source in &global {
                    if source.alias_name == *target {
                        extras.push(ModelEndpoint {
                            alias_name: alias.clone(),
                            name: source.name.clone(),
                            provider_name: source.provider_name.clone(),
                            priority: source.priority,
                            protocol: source.protocol,
                            base_url: source.base_url.clone(),
                            adapter_kind: source.adapter_kind,
                        });
                    }
                }
            }
            if !extras.is_empty() {
                key_endpoints.insert(key.id.clone(), extras);
            }
        }

        (global, key_endpoints)
    }

    pub fn provider_count(&self) -> usize {
        self.raw_config
            .providers
            .iter()
            .filter(|provider| provider.is_enabled() && provider.has_enabled_endpoint())
            .count()
    }

    pub fn auth_key_for_secret(&self, secret: &str) -> Option<AuthKey> {
        self.api_keys_by_secret.get(secret).cloned()
    }

    pub fn providers(&self) -> Vec<ProviderView> {
        self.raw_config
            .providers
            .iter()
            .map(provider_view)
            .collect()
    }

    pub fn auth_keys(&self) -> Vec<AuthKeyView> {
        self.raw_config.keys.iter().map(auth_key_view).collect()
    }

    pub fn aliases(&self) -> Vec<AliasView> {
        let mut values = Vec::new();
        for provider in &self.raw_config.providers {
            for model in &provider.models {
                for alias in &model.aliases {
                    values.push(AliasView {
                        alias: alias.clone(),
                        target_model: model.name.clone(),
                        provider_name: Some(provider.name.clone()),
                    });
                }
            }
        }
        values.sort_by(|left, right| {
            left.provider_name
                .cmp(&right.provider_name)
                .then_with(|| left.target_model.cmp(&right.target_model))
                .then_with(|| left.alias.cmp(&right.alias))
        });
        values
    }

    pub fn find_provider(&self, name: &str) -> Option<&ProviderConfig> {
        self.raw_config
            .providers
            .iter()
            .find(|provider| provider.name == name)
    }

    pub fn endpoints_for_alias(&self, alias: &str, key: Option<&AuthKey>) -> Vec<ModelEndpoint> {
        let mut results = Vec::new();
        let provider_allowed = |provider_name: &str| {
            key.map(|key| {
                key.allowed_providers.is_empty()
                    || key
                        .allowed_providers
                        .iter()
                        .any(|allowed_provider| allowed_provider == provider_name)
            })
            .unwrap_or(true)
        };
        if let Some(key) = key {
            if let Some(extras) = self.key_endpoints.get(&key.id) {
                for ep in extras {
                    if provider_allowed(&ep.provider_name)
                        && (ep.alias_name == alias
                            || (ep.alias_name.contains('*')
                                && wildcard_match(&ep.alias_name, alias)))
                    {
                        results.push(ep.clone());
                    }
                }
            }
        }
        for ep in &self.model_endpoints {
            if provider_allowed(&ep.provider_name)
                && (ep.alias_name == alias
                    || (ep.alias_name.contains('*') && wildcard_match(&ep.alias_name, alias)))
            {
                results.push(ep.clone());
            }
        }
        results
    }

    pub fn has_endpoint(&self, alias: &str, key: Option<&AuthKey>) -> bool {
        !self.endpoints_for_alias(alias, key).is_empty()
    }

    pub fn has_endpoint_for_protocols(
        &self,
        alias: &str,
        key: Option<&AuthKey>,
        protocols: &[ProviderProtocol],
    ) -> bool {
        self.endpoints_for_alias(alias, key)
            .into_iter()
            .any(|endpoint| protocols.contains(&endpoint.protocol))
    }

    pub fn provider_supports_protocol(
        &self,
        provider_name: &str,
        protocol: ProviderProtocol,
    ) -> bool {
        self.find_provider(provider_name)
            .map(|provider| provider.supports_protocol(protocol))
            .unwrap_or(false)
    }

    pub fn model_catalog(&self) -> Vec<ModelCatalogEntry> {
        let mut values = Vec::new();
        for provider in &self.raw_config.providers {
            if !provider.is_enabled() || !provider.has_enabled_endpoint() {
                continue;
            }
            for model in provider.models.iter().filter(|model| model.is_enabled()) {
                let aliases = model.aliases.clone();
                let selectable_names = model.public_names();
                for selectable_name in &selectable_names {
                    values.push(ModelCatalogEntry {
                        name: selectable_name.clone(),
                        kind: "model".to_string(),
                        provider_name: Some(provider.name.clone()),
                        protocols: provider.supported_protocols(),
                        target_model: Some(model.name.clone()),
                        model_name: model.name.clone(),
                        aliases: aliases.clone(),
                        selectable_names: vec![selectable_name.clone()],
                    });
                }
            }
        }

        values.sort_by(|left, right| {
            left.provider_name
                .cmp(&right.provider_name)
                .then_with(|| left.model_name.cmp(&right.model_name))
                .then_with(|| left.name.cmp(&right.name))
        });
        values
    }

    pub fn key_visible_model_names(&self) -> HashSet<String> {
        self.raw_config.key_visible_model_names()
    }

    pub fn provider_model_pricing(
        &self,
        provider_name: &str,
        model_name: &str,
    ) -> Option<ModelPricing> {
        self.find_provider(provider_name).and_then(|provider| {
            provider
                .models
                .iter()
                .find(|rule| rule.name == model_name)
                .and_then(|rule| rule.pricing.clone())
        })
    }
}

fn provider_adapter_kind(provider: &ProviderConfig) -> &'static str {
    provider
        .endpoints
        .first()
        .map(|ep| match ep.protocol {
            ProviderProtocol::Messages => "anthropic",
            ProviderProtocol::Completions
            | ProviderProtocol::Responses
            | ProviderProtocol::Embeddings => "openai",
        })
        .unwrap_or("openai")
}

fn provider_view(provider: &ProviderConfig) -> ProviderView {
    ProviderView {
        name: provider.name.clone(),
        api_key: provider.api_key.clone(),
        models: provider.models.clone(),
        endpoints: provider.endpoints.clone(),
        headers: provider.headers.clone(),
        status: provider.status.clone(),
        priority: provider.priority,
    }
}

fn auth_key_view(key: &AuthKey) -> AuthKeyView {
    AuthKeyView {
        id: key.id.clone(),
        name: key.name.clone(),
        key: key.key.clone(),
        status: key.status.clone(),
        labels: key.labels.clone(),
        allowed_models: key.models.clone(),
        model_aliases: key.model_aliases.clone(),
        allowed_providers: key.allowed_providers.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        AppConfig, CostConfig, EndpointConfig, ModelRule, ProviderConfig, RetryConfig,
        RoutingConfig, StickyConfig, UpstreamConfig,
    };

    fn test_config(provider_name: &str) -> AppConfig {
        AppConfig {
            providers: vec![ProviderConfig {
                name: provider_name.to_string(),
                api_key: "secret".to_string(),
                models: vec![ModelRule::enabled("gpt-4o")],
                endpoints: vec![EndpointConfig {
                    protocol: ProviderProtocol::Completions,
                    base_url: "https://api.example.com/v1/chat/completions".to_string(),
                }],
                headers: HashMap::new(),
                status: "enabled".to_string(),
                priority: 1,
            }],
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
    fn snapshot_refreshes_when_file_changes() {
        let temp_dir =
            std::env::temp_dir().join(format!("rcpa-config-service-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let config_path = temp_dir.join("config.yaml");

        std::fs::write(
            &config_path,
            serde_yaml::to_string(&test_config("provider-a")).unwrap(),
        )
        .unwrap();

        let service = ConfigService::new(&config_path).unwrap();
        assert!(service.snapshot().find_provider("provider-a").is_some());

        std::fs::write(
            &config_path,
            serde_yaml::to_string(&test_config("provider-bb")).unwrap(),
        )
        .unwrap();

        let snapshot = service.snapshot();
        assert!(snapshot.find_provider("provider-a").is_none());
        assert!(snapshot.find_provider("provider-bb").is_some());

        std::fs::remove_dir_all(temp_dir).unwrap();
    }
}
