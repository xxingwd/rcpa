use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use rand::{distributions::Alphanumeric, Rng};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub providers: Vec<ProviderConfig>,
    pub routing: RoutingConfig,
    pub retry: RetryConfig,
    pub cost: CostConfig,
    pub admin: AdminConfig,
    pub auth: AuthConfig,
    pub database: DatabaseConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub tls: TlsConfig,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TlsConfig {
    pub enabled: bool,
    pub cert_file: String,
    pub key_file: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
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
        self.is_enabled() && self.models.iter().any(|rule| rule.public_matches(model))
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
pub struct RoutingConfig {
    pub strategy: String,
    pub sticky: StickyConfig,
    pub default_model: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StickyConfig {
    pub enabled: bool,
    pub ttl_secs: u64,
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
pub struct AuthConfig {
    pub enabled: bool,
    pub keys: Vec<AuthKey>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AdminConfig {
    pub token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthKey {
    pub id: String,
    pub name: Option<String>,
    pub key: String,
    pub models: Vec<ModelRule>,
    pub model_aliases: HashMap<String, String>,
    pub status: String,
    pub labels: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    pub path: String,
}

#[derive(Debug, Clone)]
pub struct BootstrapConfigInfo {
    pub path: PathBuf,
    pub database_path: PathBuf,
    pub log_dir: PathBuf,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            path: default_db_path(),
        }
    }
}

fn default_db_path() -> String {
    "rcpa.db".to_string()
}

pub fn data_dir_for_config(path: &Path) -> PathBuf {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

impl AppConfig {
    pub fn ensure_config_file(path: &Path) -> anyhow::Result<Option<BootstrapConfigInfo>> {
        if path.exists() {
            return Ok(None);
        }

        let data_dir = data_dir_for_config(path);
        std::fs::create_dir_all(&data_dir)?;
        let database_path = data_dir.join("rcpa.db");
        let log_dir = data_dir.join("logs");
        std::fs::create_dir_all(&log_dir)?;

        let config = Self::bootstrap_config(database_path.clone());
        let content = serde_yaml::to_string(&config)?;
        std::fs::write(path, content)?;

        Ok(Some(BootstrapConfigInfo {
            path: path.to_path_buf(),
            database_path,
            log_dir,
        }))
    }

    fn bootstrap_config(database_path: PathBuf) -> Self {
        Self {
            server: ServerConfig {
                host: "0.0.0.0".into(),
                port: 15000,
                tls: TlsConfig {
                    enabled: false,
                    cert_file: String::new(),
                    key_file: String::new(),
                },
            },
            providers: Vec::new(),
            routing: RoutingConfig {
                strategy: "weighted_round_robin".into(),
                sticky: StickyConfig {
                    enabled: true,
                    ttl_secs: 300,
                },
                default_model: None,
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
            admin: AdminConfig {
                token: random_secret("admin"),
            },
            auth: AuthConfig {
                enabled: true,
                keys: vec![AuthKey {
                    id: "bootstrap".into(),
                    name: Some("bootstrap".into()),
                    key: random_secret("rcpa"),
                    models: Vec::new(),
                    model_aliases: HashMap::new(),
                    status: "enabled".into(),
                    labels: Some("bootstrap".into()),
                }],
            },
            database: DatabaseConfig {
                path: database_path.to_string_lossy().into_owned(),
            },
        }
    }

    pub fn key_visible_model_names(&self) -> HashSet<String> {
        let mut names = HashSet::new();
        for provider in self
            .providers
            .iter()
            .filter(|provider| provider.is_enabled())
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

fn random_secret(prefix: &str) -> String {
    let random: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(32)
        .map(char::from)
        .collect();
    format!("{}_{}", prefix, random)
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
        let content = std::fs::read_to_string(path)?;
        Self::from_yaml_expanded(&content)
    }

    pub fn load_raw(path: &std::path::Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
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
        if self.admin.token.trim().is_empty() {
            anyhow::bail!("Admin token cannot be empty");
        }

        for provider in &self.providers {
            validate_model_name(&provider.name, "provider name")?;
            match provider.protocol.as_str() {
                "completions" | "responses" | "messages" => {}
                other => anyhow::bail!(
                    "Provider '{}' has invalid protocol '{}'; expected completions, responses, or messages",
                    provider.name,
                    other
                ),
            }
            match provider.status.as_str() {
                "enabled" | "disabled" => {}
                other => anyhow::bail!(
                    "Provider '{}' has invalid status '{}'",
                    provider.name,
                    other
                ),
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

        if self.auth.enabled {
            for key in &self.auth.keys {
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
        std::env::set_var("RCPA_PROVIDER_API_KEY", "provider-test-secret");
        std::env::set_var("RCPA_ADMIN_TOKEN", "admin-test-token");
        std::env::set_var("RCPA_CLIENT_API_KEY", "client-test-token");

        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("config.example.yaml");
        let config = AppConfig::load(path.to_str().unwrap()).unwrap();
        assert_eq!(config.providers.len(), 1);
        assert_eq!(config.providers[0].protocol, "completions");
        assert_eq!(
            config.providers[0].actual_model_for("public-model-name"),
            Some("upstream-model-name".into())
        );
        assert_eq!(config.auth.keys.len(), 1);
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
        assert_eq!(info.database_path, temp_dir.join("rcpa.db"));
        assert_eq!(info.log_dir, temp_dir.join("logs"));
        assert!(config_path.exists());
        assert!(info.log_dir.exists());

        let config = AppConfig::load_raw(&config_path).unwrap();
        assert_eq!(
            config.database.path,
            temp_dir.join("rcpa.db").to_string_lossy()
        );
        assert!(config.providers.is_empty());
        assert!(!config.admin.token.is_empty());
        assert_eq!(config.auth.keys.len(), 1);

        assert!(AppConfig::ensure_config_file(&config_path)
            .unwrap()
            .is_none());
        std::fs::remove_dir_all(temp_dir).unwrap();
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

    fn test_provider() -> ProviderConfig {
        ProviderConfig {
            name: "primary-provider".into(),
            protocol: "completions".into(),
            base_url: "https://api.example.com".into(),
            api_key: "test-secret".into(),
            models: vec![ModelRule::enabled("gpt-4o")],
            weight: 10,
            max_connections: 100,
            timeout_secs: 300,
            headers: HashMap::new(),
            api_version: None,
            status: "enabled".into(),
            priority: 0,
            group: "default".into(),
        }
    }

    fn test_config() -> AppConfig {
        AppConfig {
            server: ServerConfig {
                host: "0.0.0.0".into(),
                port: 8080,
                tls: TlsConfig::default(),
            },
            providers: vec![],
            routing: RoutingConfig {
                strategy: "round_robin".into(),
                sticky: StickyConfig::default(),
                default_model: None,
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
            admin: AdminConfig {
                token: "admin-token".into(),
            },
            auth: AuthConfig {
                enabled: false,
                keys: vec![],
            },
            database: DatabaseConfig::default(),
        }
    }
}
