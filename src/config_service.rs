use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use arc_swap::ArcSwap;
use parking_lot::Mutex;
use serde::Serialize;

use crate::config::{wildcard_match, AppConfig, AuthKey, ModelPricing, ModelRule, ProviderConfig};
use crate::provider::ProviderRegistry;

#[derive(Clone)]
pub struct ConfigService {
    path: PathBuf,
    snapshot: Arc<ArcSwap<ConfigSnapshot>>,
    write_lock: Arc<Mutex<()>>,
}

pub struct ConfigSnapshot {
    pub raw_config: AppConfig,
    pub config: AppConfig,
    pub registry: ProviderRegistry,
    pub provider_routes: HashMap<String, Vec<String>>,
    pub provider_weights: HashMap<String, u32>,
    api_keys_by_secret: HashMap<String, AuthKey>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderView {
    pub name: String,
    pub protocol: String,
    pub base_url: String,
    pub api_key: String,
    pub models: Vec<ModelRule>,
    pub weight: u32,
    pub max_connections: usize,
    pub timeout_secs: u64,
    pub headers: HashMap<String, String>,
    pub api_version: Option<String>,
    pub status: String,
    pub priority: i64,
    pub group: String,
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
    pub protocol: Option<String>,
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
        let path = path.as_ref().to_path_buf();
        let raw_config = AppConfig::load_raw(&path)?;
        let snapshot = ConfigSnapshot::build(raw_config)?;
        Ok(Self {
            path,
            snapshot: Arc::new(ArcSwap::from_pointee(snapshot)),
            write_lock: Arc::new(Mutex::new(())),
        })
    }

    pub fn from_config(config: AppConfig) -> anyhow::Result<Self> {
        let snapshot = ConfigSnapshot::build(config)?;
        Ok(Self {
            path: PathBuf::from("config.yaml"),
            snapshot: Arc::new(ArcSwap::from_pointee(snapshot)),
            write_lock: Arc::new(Mutex::new(())),
        })
    }

    pub fn snapshot(&self) -> Arc<ConfigSnapshot> {
        self.snapshot.load_full()
    }

    pub fn path_display(&self) -> String {
        self.path.display().to_string()
    }

    pub fn read_raw_yaml(&self) -> anyhow::Result<String> {
        Ok(std::fs::read_to_string(&self.path)?)
    }

    pub fn replace_raw_yaml(&self, content: &str) -> anyhow::Result<Arc<ConfigSnapshot>> {
        let _guard = self.write_lock.lock();
        let raw_config: AppConfig = serde_yaml::from_str(content)?;
        raw_config.validate()?;
        let next_snapshot = ConfigSnapshot::build(raw_config)?;
        let tmp = self.path.with_extension("yaml.tmp");
        std::fs::write(&tmp, content)?;
        std::fs::rename(tmp, &self.path)?;
        let next = Arc::new(next_snapshot);
        self.snapshot.store(next.clone());
        Ok(next)
    }

    pub fn update<F>(&self, f: F) -> anyhow::Result<Arc<ConfigSnapshot>>
    where
        F: FnOnce(&mut AppConfig) -> anyhow::Result<()>,
    {
        let _guard = self.write_lock.lock();
        let mut raw_config = AppConfig::load_raw(&self.path)?;
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
        let tmp = self.path.with_extension("yaml.tmp");
        std::fs::write(&tmp, content)?;
        std::fs::rename(tmp, &self.path)?;
        Ok(())
    }
}

impl ConfigSnapshot {
    pub fn build(raw_config: AppConfig) -> anyhow::Result<Self> {
        let config = raw_config.expanded()?;
        let registry = ProviderRegistry::from_config(&config)?;
        let (provider_routes, provider_weights) = Self::build_provider_routes(&config);
        let api_keys_by_secret = config
            .auth
            .keys
            .iter()
            .filter(|key| key.status == "enabled")
            .map(|key| (key.key.clone(), key.clone()))
            .collect();

        Ok(Self {
            raw_config,
            config,
            registry,
            provider_routes,
            provider_weights,
            api_keys_by_secret,
        })
    }

    fn build_provider_routes(
        config: &AppConfig,
    ) -> (HashMap<String, Vec<String>>, HashMap<String, u32>) {
        let mut routes: HashMap<String, Vec<String>> = HashMap::new();
        let mut weights = HashMap::new();

        for provider in &config.providers {
            if !provider.is_enabled() {
                continue;
            }
            weights.insert(provider.name.clone(), provider.weight);
            for model in provider.models.iter().filter(|model| model.is_enabled()) {
                for public_name in model.public_names() {
                    routes
                        .entry(public_name)
                        .or_default()
                        .push(provider.name.clone());
                }
            }
        }

        (routes, weights)
    }

    pub fn provider_count(&self) -> usize {
        self.config
            .providers
            .iter()
            .filter(|provider| provider.is_enabled())
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
        self.raw_config
            .auth
            .keys
            .iter()
            .map(auth_key_view)
            .collect()
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
        self.config
            .providers
            .iter()
            .find(|provider| provider.name == name)
    }

    pub fn enabled_provider_can_serve_model(&self, provider_name: &str, model: &str) -> bool {
        self.find_provider(provider_name)
            .map(|provider| provider.can_serve_model(model))
            .unwrap_or(false)
    }

    pub fn provider_protocol(&self, provider_name: &str) -> Option<&str> {
        self.find_provider(provider_name)
            .map(|provider| provider.protocol.as_str())
    }

    pub fn providers_for_model(&self, model: &str) -> Vec<String> {
        if let Some(providers) = self.provider_routes.get(model) {
            return providers.clone();
        }

        for (pattern, providers) in &self.provider_routes {
            if pattern.contains('*') && wildcard_match(pattern, model) {
                return providers.clone();
            }
        }

        Vec::new()
    }

    pub fn has_model(&self, model: &str) -> bool {
        !self.providers_for_model(model).is_empty()
    }

    pub fn list_models(&self) -> Vec<(String, String)> {
        let mut values = Vec::new();
        for provider in &self.config.providers {
            if !provider.is_enabled() {
                continue;
            }
            for model in provider.models.iter().filter(|model| model.is_enabled()) {
                for public_name in model.public_names() {
                    values.push((public_name, provider.name.clone()));
                }
            }
        }
        values
    }

    pub fn model_catalog(&self) -> Vec<ModelCatalogEntry> {
        let mut values = Vec::new();
        for provider in &self.raw_config.providers {
            if !provider.is_enabled() {
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
                        protocol: Some(provider.protocol.clone()),
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
        self.config.key_visible_model_names()
    }

    pub fn model_catalog_entry_for_name(&self, name: &str) -> Option<ModelCatalogEntry> {
        self.model_catalog()
            .into_iter()
            .find(|entry| entry.selectable_names.iter().any(|item| item == name))
    }

    pub fn visible_model_owner(&self, name: &str) -> Option<String> {
        self.model_catalog()
            .into_iter()
            .find(|entry| entry.selectable_names.iter().any(|item| item == name))
            .and_then(|entry| entry.provider_name)
    }

    pub fn valid_model_target_names(&self) -> HashSet<String> {
        self.key_visible_model_names()
    }

    pub fn resolve_key_alias(
        &self,
        requested: &str,
        key: &AuthKey,
    ) -> Option<(String, Option<String>)> {
        let target = key.model_aliases.get(requested)?;
        Some((target.clone(), None))
    }

    pub fn provider_model_pricing(&self, provider_name: &str, model: &str) -> Option<ModelPricing> {
        self.find_provider(provider_name).and_then(|provider| {
            provider
                .models
                .iter()
                .find(|rule| rule.matches(model) || rule.public_matches(model))
                .and_then(|rule| rule.pricing.clone())
        })
    }
}

fn provider_view(provider: &ProviderConfig) -> ProviderView {
    ProviderView {
        name: provider.name.clone(),
        protocol: provider.protocol.clone(),
        base_url: provider.base_url.clone(),
        api_key: provider.api_key.clone(),
        models: provider.models.clone(),
        weight: provider.weight,
        max_connections: provider.max_connections,
        timeout_secs: provider.timeout_secs,
        headers: provider.headers.clone(),
        api_version: provider.api_version.clone(),
        status: provider.status.clone(),
        priority: provider.priority,
        group: provider.group.clone(),
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
    }
}

pub fn normalize_model_rules(models: Vec<String>) -> Vec<ModelRule> {
    models.into_iter().map(ModelRule::enabled).collect()
}

pub fn model_names(models: &[ModelRule]) -> HashSet<String> {
    models.iter().map(|model| model.name.clone()).collect()
}
