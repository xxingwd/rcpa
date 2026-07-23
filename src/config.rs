use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

pub const CONFIG_FILE_NAME: &str = "config.yaml";
pub const SQLITE_FILE_NAME: &str = "rcpa.db";
pub const LOG_DIR_NAME: &str = "logs";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppConfig {
    pub providers: Vec<ProviderConfig>,
    pub upstream: UpstreamConfig,
    pub routing: RoutingConfig,
    pub retry: RetryConfig,
    pub cost: CostConfig,
    pub keys: Vec<AuthKey>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ProviderProtocol {
    Completions,
    Responses,
    Messages,
    Embeddings,
}

impl std::fmt::Display for ProviderProtocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProviderProtocol::Completions => write!(f, "completions"),
            ProviderProtocol::Responses => write!(f, "responses"),
            ProviderProtocol::Messages => write!(f, "messages"),
            ProviderProtocol::Embeddings => write!(f, "embeddings"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderConfig {
    pub name: String,
    pub api_key: String,
    pub models: Vec<ModelRule>,
    pub endpoints: Vec<EndpointConfig>,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(default = "default_status")]
    pub status: String,
    pub priority: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EndpointConfig {
    pub protocol: ProviderProtocol,
    /// Full upstream HTTP endpoint URL. RCPA sends requests to this URL as-is.
    pub base_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRule {
    pub name: String,
    pub status: String,
    pub pricing: Option<ModelPricing>,
    pub aliases: Vec<String>,
}

impl ModelRule {
    pub fn enabled(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: default_status(),
            pricing: None,
            aliases: Vec::new(),
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.status == "enabled"
    }

    pub fn matches(&self, model: &str) -> bool {
        self.is_enabled()
            && if self.name.contains('*') {
                wildcard_match(&self.name, model)
            } else {
                self.name == model
            }
    }

    pub fn public_names(&self) -> Vec<String> {
        if self.aliases.is_empty() {
            vec![self.name.clone()]
        } else {
            self.aliases.clone()
        }
    }

    pub fn public_matches(&self, model: &str) -> bool {
        self.is_enabled()
            && if self.aliases.is_empty() {
                self.matches(model)
            } else {
                self.aliases.iter().any(|alias| alias == model)
            }
    }

    pub fn actual_model_for(&self, model: &str) -> Option<String> {
        if !self.public_matches(model) {
            return None;
        }

        if self.aliases.is_empty() && self.name.contains('*') {
            Some(model.to_string())
        } else {
            Some(self.name.clone())
        }
    }
}

impl ProviderConfig {
    pub fn is_enabled(&self) -> bool {
        self.status == "enabled"
    }

    pub fn has_enabled_endpoint(&self) -> bool {
        self.is_enabled() && !self.endpoints.is_empty()
    }

    pub fn supports_protocol(&self, protocol: ProviderProtocol) -> bool {
        self.is_enabled() && self.endpoints.iter().any(|e| e.protocol == protocol)
    }

    pub fn supported_protocols(&self) -> Vec<ProviderProtocol> {
        if self.is_enabled() {
            self.endpoints.iter().map(|e| e.protocol).collect()
        } else {
            Vec::new()
        }
    }

    pub fn base_url_for_protocol(&self, protocol: ProviderProtocol) -> Option<&str> {
        if !self.is_enabled() {
            return None;
        }
        self.endpoints
            .iter()
            .find(|endpoint| endpoint.protocol == protocol)
            .map(|endpoint| endpoint.base_url.as_str())
    }

    pub fn enabled_model_names(&self) -> Vec<String> {
        self.models
            .iter()
            .filter(|model| model.is_enabled())
            .flat_map(|model| model.public_names())
            .collect()
    }

    pub fn enabled_model_mappings(&self) -> HashMap<String, String> {
        let mut mappings = HashMap::new();
        for model in self.models.iter().filter(|model| model.is_enabled()) {
            for public_name in model.public_names() {
                mappings.insert(public_name, model.name.clone());
            }
        }
        mappings
    }

    pub fn enabled_pricing(&self) -> HashMap<String, ModelPricing> {
        self.models
            .iter()
            .filter(|model| model.is_enabled())
            .filter_map(|model| {
                model
                    .pricing
                    .as_ref()
                    .map(|pricing| (model.name.clone(), pricing.clone()))
            })
            .collect()
    }

    pub fn can_serve_model(&self, model: &str) -> bool {
        self.is_enabled()
            && self.has_enabled_endpoint()
            && self.models.iter().any(|rule| rule.public_matches(model))
    }

    pub fn actual_model_for(&self, model: &str) -> Option<String> {
        if !self.is_enabled() {
            return None;
        }
        self.models
            .iter()
            .find_map(|rule| rule.actual_model_for(model))
    }
}

pub fn default_status() -> String {
    "enabled".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpstreamConfig {
    pub timeout_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoutingConfig {
    pub sticky: StickyConfig,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StickyConfig {
    pub enabled: bool,
    pub ttl_secs: u64,
    #[serde(default)]
    pub fallback_retry_interval_secs: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    pub max_attempts: u32,
    pub initial_backoff_ms: u64,
    pub max_backoff_ms: u64,
    pub retryable_statuses: Vec<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostConfig {
    pub currency: String,
    pub default_input_per_1k: f64,
    pub default_output_per_1k: f64,
    pub models: HashMap<String, ModelPricing>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPricing {
    pub input_per_1k: f64,
    pub output_per_1k: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthKey {
    pub id: String,
    pub name: Option<String>,
    pub key: String,
    pub models: Vec<ModelRule>,
    pub model_aliases: HashMap<String, String>,
    #[serde(default)]
    pub allowed_providers: Vec<String>,
    pub status: String,
    pub labels: Option<String>,
}

#[derive(Debug, Clone)]
pub struct BootstrapConfigInfo {
    pub path: PathBuf,
    pub data_dir: PathBuf,
    pub sqlite_path: PathBuf,
    pub log_dir: PathBuf,
}

pub fn expand_tilde(path: &Path) -> PathBuf {
    if path.starts_with("~") {
        if let Some(home) = dirs::home_dir() {
            return home.join(path.strip_prefix("~").unwrap());
        }
    }
    path.to_path_buf()
}

pub fn config_path_for_data_dir(data_dir: &Path) -> PathBuf {
    expand_tilde(data_dir).join(CONFIG_FILE_NAME)
}

pub fn sqlite_path_for_data_dir(data_dir: &Path) -> PathBuf {
    expand_tilde(data_dir).join(SQLITE_FILE_NAME)
}

pub fn log_dir_for_data_dir(data_dir: &Path) -> PathBuf {
    expand_tilde(data_dir).join(LOG_DIR_NAME)
}

impl AppConfig {
    pub fn ensure_config_file(path: &Path) -> anyhow::Result<Option<BootstrapConfigInfo>> {
        let expanded_path = expand_tilde(path);
        if expanded_path.exists() {
            return Ok(None);
        }

        let data_dir = expanded_path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        std::fs::create_dir_all(&data_dir)?;
        let sqlite_path = sqlite_path_for_data_dir(&data_dir);
        let log_dir = log_dir_for_data_dir(&data_dir);
        std::fs::create_dir_all(&log_dir)?;

        let config = Self::bootstrap_config();
        let content = serde_yaml::to_string(&config)?;
        std::fs::write(&expanded_path, content)?;

        Ok(Some(BootstrapConfigInfo {
            path: expanded_path,
            data_dir,
            sqlite_path,
            log_dir,
        }))
    }

    fn bootstrap_config() -> Self {
        Self {
            providers: Vec::new(),
            upstream: UpstreamConfig { timeout_secs: 300 },
            routing: RoutingConfig {
                sticky: StickyConfig {
                    enabled: true,
                    ttl_secs: 300,
                    fallback_retry_interval_secs: None,
                },
            },
            retry: RetryConfig {
                max_attempts: 3,
                initial_backoff_ms: 100,
                max_backoff_ms: 10000,
                retryable_statuses: vec![429, 502, 503, 504],
            },
            cost: CostConfig {
                currency: "USD".into(),
                default_input_per_1k: 0.0,
                default_output_per_1k: 0.0,
                models: HashMap::new(),
            },
            keys: Vec::new(),
        }
    }

    pub fn key_visible_model_names(&self) -> HashSet<String> {
        let mut names = HashSet::new();
        for provider in self
            .providers
            .iter()
            .filter(|provider| provider.is_enabled() && provider.has_enabled_endpoint())
        {
            for model in provider.models.iter().filter(|model| model.is_enabled()) {
                names.extend(model.public_names());
            }
        }
        names
    }

    pub fn key_can_use_model(key: &AuthKey, model: &str) -> bool {
        if key.models.is_empty() {
            return true;
        }
        key.models
            .iter()
            .any(|rule| rule.is_enabled() && rule.name == model)
    }
}

pub fn wildcard_match(pattern: &str, input: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if let Some(prefix) = pattern.strip_suffix('*') {
        return input.starts_with(prefix);
    }
    pattern == input
}

impl AppConfig {
    pub fn load(path: &str) -> anyhow::Result<Self> {
        let expanded_path = expand_tilde(Path::new(path));
        let content = std::fs::read_to_string(expanded_path)?;
        Self::from_yaml_expanded(&content)
    }

    pub fn load_raw(path: &std::path::Path) -> anyhow::Result<Self> {
        let expanded_path = expand_tilde(path);
        let content = std::fs::read_to_string(expanded_path)?;
        let config: Self = serde_yaml::from_str(&content)?;
        config.validate()?;
        Ok(config)
    }

    pub fn expanded(&self) -> anyhow::Result<Self> {
        let raw = serde_yaml::to_string(self)?;
        Self::from_yaml_expanded(&raw)
    }

    pub fn from_yaml_expanded(content: &str) -> anyhow::Result<Self> {
        let expanded = Self::expand_env_vars(content)?;
        let config: Self = serde_yaml::from_str(&expanded)?;
        config.validate()?;
        Ok(config)
    }

    pub fn expand_env_vars(content: &str) -> anyhow::Result<String> {
        let re = regex::Regex::new(r"\$\{([^}]+)\}").unwrap();
        let mut missing_var = None;
        let expanded = re.replace_all(content, |caps: &regex::Captures| {
            let var_name = &caps[1];
            match std::env::var(var_name) {
                Ok(value) => value,
                Err(_) => {
                    missing_var = Some(var_name.to_string());
                    caps[0].to_string()
                }
            }
        });
        if let Some(var_name) = missing_var {
            anyhow::bail!("Missing environment variable '{}'", var_name);
        }
        Ok(expanded.to_string())
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        validate_upstream_config(&self.upstream)?;
        validate_routing_config(&self.routing)?;
        validate_retry_config(&self.retry)?;
        validate_cost_config(&self.cost)?;

        let mut seen_names = HashSet::new();
        for provider in &self.providers {
            validate_provider_config(provider)?;
            if !seen_names.insert(provider.name.clone()) {
                anyhow::bail!("Duplicate provider name '{}'", provider.name);
            }
            for model in &provider.models {
                validate_model_name(&model.name, "provider model")?;
                match model.status.as_str() {
                    "enabled" | "disabled" => {}
                    other => anyhow::bail!(
                        "Provider '{}' model '{}' has invalid status '{}'",
                        provider.name,
                        model.name,
                        other
                    ),
                }
                if model.name.contains('*') && !model.aliases.is_empty() {
                    anyhow::bail!(
                        "Provider '{}' wildcard model '{}' cannot define aliases",
                        provider.name,
                        model.name
                    );
                }
                if let Some(pricing) = &model.pricing {
                    validate_pricing(
                        pricing,
                        &format!("Provider '{}' model '{}'", provider.name, model.name),
                    )?;
                }
                let mut seen_aliases = HashSet::new();
                for alias in &model.aliases {
                    validate_model_name(alias, "provider model alias")?;
                    if !seen_aliases.insert(alias) {
                        anyhow::bail!(
                            "Provider '{}' model '{}' defines duplicate alias '{}'",
                            provider.name,
                            model.name,
                            alias
                        );
                    }
                }
            }
        }

        let platform_names = self.key_visible_model_names();
        let provider_names: HashSet<&String> = self.providers.iter().map(|p| &p.name).collect();

        for key in &self.keys {
            if key.id.trim().is_empty() {
                anyhow::bail!("Auth key id cannot be empty");
            }
            if key.key.trim().is_empty() {
                anyhow::bail!("Auth key '{}' has empty key", key.id);
            }
            match key.status.as_str() {
                "enabled" | "disabled" => {}
                other => anyhow::bail!("Auth key '{}' has invalid status '{}'", key.id, other),
            }
            let mut seen_allowed_providers = HashSet::new();
            for provider_name in &key.allowed_providers {
                validate_model_name(provider_name, "allowed provider")?;
                if !seen_allowed_providers.insert(provider_name) {
                    anyhow::bail!(
                        "Auth key '{}' allows duplicate provider '{}'",
                        key.id,
                        provider_name
                    );
                }
                if !provider_names.contains(provider_name) {
                    anyhow::bail!(
                        "Auth key '{}' allows unknown provider '{}'",
                        key.id,
                        provider_name
                    );
                }
            }
            let key_aliases: HashSet<&String> = key.model_aliases.keys().collect();
            for (alias, target) in &key.model_aliases {
                validate_model_name(alias, "key model alias")?;
                validate_model_name(target, "key model alias target")?;
                if platform_names.contains(alias) {
                    anyhow::bail!(
                        "Key '{}' model alias '{}' conflicts with a platform model name",
                        key.id,
                        alias
                    );
                }
                if !platform_names.contains(target) {
                    anyhow::bail!(
                        "Key '{}' model alias '{}' targets unknown platform model '{}'",
                        key.id,
                        alias,
                        target
                    );
                }
            }
            for model in &key.models {
                validate_model_name(&model.name, "allowed model")?;
                match model.status.as_str() {
                    "enabled" | "disabled" => {}
                    other => anyhow::bail!(
                        "Auth key '{}' model '{}' has invalid status '{}'",
                        key.id,
                        model.name,
                        other
                    ),
                }
                if !platform_names.contains(&model.name) && !key_aliases.contains(&model.name) {
                    anyhow::bail!(
                        "Auth key '{}' allows unknown model '{}'",
                        key.id,
                        model.name
                    );
                }
            }
        }

        Ok(())
    }
}

fn validate_model_name(value: &str, field: &str) -> anyhow::Result<()> {
    if value.trim().is_empty() {
        anyhow::bail!("{} cannot be empty", field);
    }
    if value.trim() != value {
        anyhow::bail!(
            "{} '{}' must not contain leading or trailing whitespace",
            field,
            value
        );
    }
    Ok(())
}

fn validate_upstream_config(upstream: &UpstreamConfig) -> anyhow::Result<()> {
    if upstream.timeout_secs == 0 {
        anyhow::bail!("Upstream timeout_secs must be greater than 0");
    }
    Ok(())
}

fn validate_routing_config(routing: &RoutingConfig) -> anyhow::Result<()> {
    if routing.sticky.enabled && routing.sticky.ttl_secs == 0 {
        anyhow::bail!("Routing sticky ttl_secs must be greater than 0 when sticky is enabled");
    }
    Ok(())
}

fn validate_retry_config(retry: &RetryConfig) -> anyhow::Result<()> {
    if retry.max_attempts == 0 {
        anyhow::bail!("Retry max_attempts must be greater than 0");
    }
    if retry.initial_backoff_ms > retry.max_backoff_ms {
        anyhow::bail!("Retry initial_backoff_ms must not exceed max_backoff_ms");
    }
    for status in &retry.retryable_statuses {
        if !(400..=599).contains(status) {
            anyhow::bail!(
                "Retry status '{}' is invalid; expected an HTTP 4xx or 5xx status",
                status
            );
        }
    }
    Ok(())
}

fn validate_cost_config(cost: &CostConfig) -> anyhow::Result<()> {
    if cost.currency.trim().is_empty() {
        anyhow::bail!("Cost currency cannot be empty");
    }
    if cost.default_input_per_1k < 0.0 {
        anyhow::bail!("Default input price must be non-negative");
    }
    if cost.default_output_per_1k < 0.0 {
        anyhow::bail!("Default output price must be non-negative");
    }
    for (model, pricing) in &cost.models {
        validate_model_name(model, "cost model")?;
        validate_pricing(pricing, &format!("Cost model '{}'", model))?;
    }
    Ok(())
}

fn validate_provider_config(provider: &ProviderConfig) -> anyhow::Result<()> {
    validate_model_name(&provider.name, "provider name")?;
    match provider.status.as_str() {
        "enabled" | "disabled" => {}
        other => anyhow::bail!(
            "Provider '{}' has invalid status '{}'",
            provider.name,
            other
        ),
    }
    if provider.api_key.trim().is_empty() {
        anyhow::bail!("Provider '{}' api_key cannot be empty", provider.name);
    }
    if provider.endpoints.is_empty() {
        anyhow::bail!(
            "Provider '{}' must declare at least one endpoint",
            provider.name
        );
    }

    let mut seen_protocols = HashSet::new();
    for endpoint in &provider.endpoints {
        if !seen_protocols.insert(endpoint.protocol) {
            anyhow::bail!(
                "Provider '{}' defines duplicate endpoint protocol '{}'",
                provider.name,
                endpoint.protocol
            );
        }
        if endpoint.base_url.trim().is_empty() {
            anyhow::bail!(
                "Provider '{}' endpoint '{}' base_url cannot be empty",
                provider.name,
                endpoint.protocol
            );
        }
    }
    Ok(())
}

fn validate_pricing(pricing: &ModelPricing, owner: &str) -> anyhow::Result<()> {
    if pricing.input_per_1k < 0.0 {
        anyhow::bail!("{} input price must be non-negative", owner);
    }
    if pricing.output_per_1k < 0.0 {
        anyhow::bail!("{} output price must be non-negative", owner);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn test_expand_env_vars() {
        let _guard = env_lock().lock().unwrap();
        std::env::set_var("TEST_KEY", "test_value");
        let result = AppConfig::expand_env_vars("hello ${TEST_KEY}").unwrap();
        assert_eq!(result, "hello test_value");
    }

    #[test]
    fn test_expand_env_vars_errors_when_missing() {
        let _guard = env_lock().lock().unwrap();
        std::env::remove_var("RCPA_MISSING_TEST_KEY");
        let err = AppConfig::expand_env_vars("hello ${RCPA_MISSING_TEST_KEY}")
            .unwrap_err()
            .to_string();
        assert!(err.contains("RCPA_MISSING_TEST_KEY"));
    }

    #[test]
    fn test_example_config_loads() {
        let _guard = env_lock().lock().unwrap();

        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("config.example.yaml");
        let config = AppConfig::load(path.to_str().unwrap()).unwrap();
        assert_eq!(config.providers.len(), 2);
        assert_eq!(config.upstream.timeout_secs, 300);
        assert_eq!(
            config.providers[0].supported_protocols(),
            vec![ProviderProtocol::Completions, ProviderProtocol::Responses,]
        );
        assert_eq!(
            config.providers[0].actual_model_for("gpt-4o"),
            Some("gpt-4o".into())
        );
        assert_eq!(
            config.providers[1].supported_protocols(),
            vec![ProviderProtocol::Messages]
        );
        assert_eq!(
            config.providers[1].actual_model_for("claude-sonnet"),
            Some("claude-sonnet-4-20250514".into())
        );
        assert_eq!(config.keys.len(), 1);
        assert_eq!(
            config.keys[0].model_aliases.get("fast").map(String::as_str),
            Some("gpt-4o")
        );
        assert_eq!(
            config.keys[0]
                .model_aliases
                .get("smart")
                .map(String::as_str),
            Some("claude-sonnet")
        );
    }

    #[test]
    fn test_completions_is_the_only_chat_completions_provider_protocol() {
        let protocol: ProviderProtocol = serde_yaml::from_str("completions").unwrap();
        assert_eq!(protocol, ProviderProtocol::Completions);

        let err = serde_yaml::from_str::<ProviderProtocol>("chat_completions")
            .unwrap_err()
            .to_string();
        assert!(err.contains("unknown variant"));
    }

    #[test]
    fn test_missing_config_is_bootstrapped() {
        let temp_dir =
            std::env::temp_dir().join(format!("rcpa-bootstrap-test-{}", uuid::Uuid::new_v4()));
        let config_path = temp_dir.join("config.yaml");

        let info = AppConfig::ensure_config_file(&config_path)
            .unwrap()
            .unwrap();
        assert_eq!(info.path, config_path);
        assert_eq!(info.data_dir, temp_dir);
        assert_eq!(info.sqlite_path, temp_dir.join("rcpa.db"));
        assert_eq!(info.log_dir, temp_dir.join("logs"));
        assert!(config_path.exists());
        assert!(info.log_dir.exists());

        let config = AppConfig::load_raw(&config_path).unwrap();
        assert!(config.providers.is_empty());
        assert!(config.keys.is_empty());
        assert_eq!(config.upstream.timeout_secs, 300);

        assert!(AppConfig::ensure_config_file(&config_path)
            .unwrap()
            .is_none());
        std::fs::remove_dir_all(temp_dir).unwrap();
    }

    #[test]
    fn test_platform_runtime_fields_are_rejected_in_yaml() {
        let yaml = r#"
server:
  host: 0.0.0.0
  port: 15000
providers: []
routing:
  sticky:
    enabled: false
    ttl_secs: 0
retry:
  max_attempts: 1
  initial_backoff_ms: 1
  max_backoff_ms: 1
  retryable_statuses:
  - 429
cost:
  currency: USD
  default_input_per_1k: 0.0
  default_output_per_1k: 0.0
  models: {}
admin:
  token: token
keys: []
database:
  path: rcpa.db
"#;
        let err = AppConfig::from_yaml_expanded(yaml).unwrap_err().to_string();
        assert!(err.contains("unknown field"));
    }

    #[test]
    fn test_legacy_auth_wrapper_is_rejected() {
        let yaml = r#"
providers: []
routing:
  sticky:
    enabled: true
    ttl_secs: 1
retry:
  max_attempts: 1
  initial_backoff_ms: 1
  max_backoff_ms: 1
  retryable_statuses:
  - 429
cost:
  currency: USD
  default_input_per_1k: 0.0
  default_output_per_1k: 0.0
  models: {}
auth:
  keys: []
"#;
        let err = AppConfig::from_yaml_expanded(yaml).unwrap_err().to_string();
        assert!(err.contains("unknown field"));
    }

    #[test]
    fn test_default_model_is_rejected() {
        let yaml = r#"
providers: []
routing:
  sticky:
    enabled: true
    ttl_secs: 1
  default_model: gpt-4o
retry:
  max_attempts: 1
  initial_backoff_ms: 1
  max_backoff_ms: 1
  retryable_statuses:
  - 429
cost:
  currency: USD
  default_input_per_1k: 0.0
  default_output_per_1k: 0.0
  models: {}
keys: []
"#;
        let err = AppConfig::from_yaml_expanded(yaml).unwrap_err().to_string();
        assert!(err.contains("unknown field"));
    }

    #[test]
    fn test_wildcard_match() {
        assert!(wildcard_match("gpt-4*", "gpt-4o"));
        assert!(wildcard_match("gpt-4*", "gpt-4o-mini"));
        assert!(!wildcard_match("gpt-4*", "gpt-3.5-turbo"));
        assert!(wildcard_match("*", "anything"));
        assert!(wildcard_match("claude-*", "claude-sonnet-4-6"));
    }

    #[test]
    fn test_key_can_use_model() {
        let key = AuthKey {
            id: "key-test".into(),
            name: None,
            key: "test-secret".into(),
            models: vec![
                ModelRule::enabled("gpt-4*"),
                ModelRule::enabled("claude-sonnet-4-6"),
            ],
            model_aliases: HashMap::new(),
            allowed_providers: Vec::new(),
            status: "enabled".into(),
            labels: None,
        };
        assert!(AppConfig::key_can_use_model(&key, "gpt-4*"));
        assert!(AppConfig::key_can_use_model(&key, "claude-sonnet-4-6"));
        assert!(!AppConfig::key_can_use_model(&key, "gpt-4o"));
        assert!(!AppConfig::key_can_use_model(&key, "gpt-4o-mini"));
        assert!(!AppConfig::key_can_use_model(&key, "gpt-3.5-turbo"));
    }

    #[test]
    fn test_disabled_model_rule_denies_model() {
        let key = AuthKey {
            id: "key-test".into(),
            name: None,
            key: "test-secret".into(),
            models: vec![ModelRule {
                name: "gpt-4o".into(),
                status: "disabled".into(),
                pricing: None,
                aliases: Vec::new(),
            }],
            model_aliases: HashMap::new(),
            allowed_providers: Vec::new(),
            status: "enabled".into(),
            labels: None,
        };
        assert!(!AppConfig::key_can_use_model(&key, "gpt-4o"));
    }

    #[test]
    fn test_provider_model_alias_is_public_name() {
        let mut config = test_config();
        let mut provider = test_provider();
        provider.models[0].aliases = vec!["my-gpt".into()];
        config.providers.push(provider);

        assert!(config.key_visible_model_names().contains("my-gpt"));
        assert!(!config.key_visible_model_names().contains("gpt-4o"));
        assert_eq!(
            config.providers[0].actual_model_for("my-gpt"),
            Some("gpt-4o".into())
        );
    }

    #[test]
    fn test_validate_empty_providers() {
        let config = test_config();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_rejects_invalid_runtime_config() {
        let mut config = test_config();
        config.upstream.timeout_secs = 0;
        let err = config.validate().unwrap_err().to_string();
        assert!(err.contains("Upstream timeout_secs"));

        let mut config = test_config();
        config.routing.sticky.enabled = true;
        config.routing.sticky.ttl_secs = 0;
        let err = config.validate().unwrap_err().to_string();
        assert!(err.contains("ttl_secs"));

        let mut config = test_config();
        config.retry.max_attempts = 0;
        let err = config.validate().unwrap_err().to_string();
        assert!(err.contains("max_attempts"));
    }

    #[test]
    fn test_validate_rejects_invalid_provider_runtime_config() {
        let mut config = test_config();
        let mut provider = test_provider();
        provider.models[0].pricing = Some(ModelPricing {
            input_per_1k: -1.0,
            output_per_1k: 0.0,
        });
        config.providers.push(provider);
        let err = config.validate().unwrap_err().to_string();
        assert!(err.contains("input price"));
    }

    #[test]
    fn test_provider_endpoints_expose_protocol_specific_base_urls() {
        let mut config = test_config();
        config.providers = vec![ProviderConfig {
            name: "supplier".into(),
            api_key: "secret".into(),
            models: vec![ModelRule::enabled("gpt-4o")],
            endpoints: vec![
                EndpointConfig {
                    protocol: ProviderProtocol::Completions,
                    base_url: "https://api.example.com/v1/chat/completions".into(),
                },
                EndpointConfig {
                    protocol: ProviderProtocol::Responses,
                    base_url: "https://api.example.com/v1/responses".into(),
                },
            ],
            headers: HashMap::new(),
            status: "enabled".into(),
            priority: 1,
        }];

        let expanded = config.expanded().unwrap();
        let provider = &expanded.providers[0];
        assert_eq!(
            provider.base_url_for_protocol(ProviderProtocol::Completions),
            Some("https://api.example.com/v1/chat/completions")
        );
        assert_eq!(
            provider.base_url_for_protocol(ProviderProtocol::Responses),
            Some("https://api.example.com/v1/responses")
        );
    }

    #[test]
    fn test_validate_rejects_duplicate_endpoint_protocols() {
        let mut config = test_config();
        let mut provider = test_provider();
        provider.endpoints.push(EndpointConfig {
            protocol: ProviderProtocol::Completions,
            base_url: "https://api-backup.example.com/v1/chat/completions".into(),
        });
        config.providers.push(provider);

        let err = config.validate().unwrap_err().to_string();
        assert!(err.contains("duplicate endpoint protocol"));
    }

    #[test]
    fn test_validate_allows_base_url_without_path() {
        let mut config = test_config();
        let mut provider = test_provider();
        provider.endpoints[0].base_url = "https://api.example.com".into();
        config.providers.push(provider);

        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_rejects_legacy_provider_protocols_and_base_url_fields() {
        let yaml = r#"
providers:
  - name: openai
    api_key: secret
    models:
      - name: gpt-4o
        status: enabled
        aliases: []
    protocols:
      - completions
    base_url: https://api.openai.com/v1/chat/completions
    headers: {}
    status: enabled
    priority: 0
upstream:
  timeout_secs: 30
routing:
  sticky:
    enabled: false
    ttl_secs: 0
retry:
  max_attempts: 1
  initial_backoff_ms: 1
  max_backoff_ms: 1
  retryable_statuses:
    - 429
cost:
  currency: USD
  default_input_per_1k: 0.0
  default_output_per_1k: 0.0
  models: {}
keys: []
"#;
        let err = AppConfig::from_yaml_expanded(yaml).unwrap_err().to_string();
        assert!(err.contains("unknown field"));
    }

    #[test]
    fn test_rejects_provider_level_timeout_after_globalization() {
        let yaml = r#"
providers:
  - name: openai
    api_key: secret
    models:
      - name: gpt-4o
        status: enabled
        aliases: []
    endpoints:
      - protocol: completions
        base_url: https://api.openai.com/v1/chat/completions
    timeout_secs: 30
    headers: {}
    status: enabled
    priority: 0
upstream:
  timeout_secs: 30
routing:
  sticky:
    enabled: false
    ttl_secs: 0
retry:
  max_attempts: 1
  initial_backoff_ms: 1
  max_backoff_ms: 1
  retryable_statuses:
    - 429
cost:
  currency: USD
  default_input_per_1k: 0.0
  default_output_per_1k: 0.0
  models: {}
keys: []
"#;
        let err = AppConfig::from_yaml_expanded(yaml).unwrap_err().to_string();
        assert!(err.contains("unknown field `timeout_secs`"));
    }

    #[test]
    fn test_rejects_missing_global_upstream_config() {
        let yaml = r#"
providers: []
routing:
  sticky:
    enabled: false
    ttl_secs: 0
retry:
  max_attempts: 1
  initial_backoff_ms: 1
  max_backoff_ms: 1
  retryable_statuses:
    - 429
cost:
  currency: USD
  default_input_per_1k: 0.0
  default_output_per_1k: 0.0
  models: {}
keys: []
"#;
        let err = AppConfig::from_yaml_expanded(yaml).unwrap_err().to_string();
        assert!(err.contains("missing field `upstream`"));
    }

    fn test_provider() -> ProviderConfig {
        ProviderConfig {
            name: "default".into(),
            api_key: "test-secret".into(),
            models: vec![ModelRule::enabled("gpt-4o")],
            endpoints: vec![EndpointConfig {
                protocol: ProviderProtocol::Completions,
                base_url: "https://api.example.com/v1/chat/completions".into(),
            }],
            headers: HashMap::new(),
            status: "enabled".into(),
            priority: 0,
        }
    }

    fn test_config() -> AppConfig {
        AppConfig {
            providers: vec![],
            upstream: UpstreamConfig { timeout_secs: 300 },
            routing: RoutingConfig {
                sticky: StickyConfig::default(),
            },
            retry: RetryConfig {
                max_attempts: 3,
                initial_backoff_ms: 100,
                max_backoff_ms: 10000,
                retryable_statuses: vec![429, 502],
            },
            cost: CostConfig {
                currency: "CNY".into(),
                default_input_per_1k: 0.0,
                default_output_per_1k: 0.0,
                models: HashMap::new(),
            },
            keys: vec![],
        }
    }
}
