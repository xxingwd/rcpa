pub mod router;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use crate::config::AppConfig;
use crate::config_service::ConfigService;
use crate::routing::sticky::StickySessions;
use crate::routing::ModelRouter;
use crate::stats::StatsCollector;

/// Shared application state accessible from all handlers.
pub struct AppState {
    pub config_service: Arc<ConfigService>,
    pub admin_token: String,
    pub router: Arc<ModelRouter>,
    pub stats: Arc<StatsCollector>,
    pub sticky_sessions: Arc<StickySessions>,
    pub store: crate::store::Store,
    pub start_time: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub admin_token: String,
    pub sqlite_path: Option<PathBuf>,
}

impl RuntimeConfig {
    pub fn new(admin_token: impl Into<String>, sqlite_path: PathBuf) -> Self {
        Self {
            admin_token: admin_token.into(),
            sqlite_path: Some(sqlite_path),
        }
    }

    pub fn in_memory(admin_token: impl Into<String>) -> Self {
        Self {
            admin_token: admin_token.into(),
            sqlite_path: None,
        }
    }
}

impl AppState {
    pub async fn new(
        config_service: Arc<ConfigService>,
        runtime: RuntimeConfig,
    ) -> anyhow::Result<Self> {
        if runtime.admin_token.trim().is_empty() {
            anyhow::bail!("Admin token cannot be empty");
        }
        let snapshot = config_service.snapshot();
        let store = match &runtime.sqlite_path {
            Some(path) => crate::store::Store::open_path(path).await?,
            None => crate::store::Store::open_in_memory().await?,
        };
        let router = ModelRouter::new(&snapshot.config)?;
        let stats = StatsCollector::new();
        let sticky_sessions = StickySessions::new(snapshot.config.routing.sticky.ttl_secs);

        Ok(Self {
            config_service,
            admin_token: runtime.admin_token,
            router: Arc::new(router),
            stats: Arc::new(stats),
            sticky_sessions: Arc::new(sticky_sessions),
            store,
            start_time: chrono::Utc::now(),
        })
    }

    pub async fn from_config(config: AppConfig) -> anyhow::Result<Self> {
        let config_service = Arc::new(ConfigService::from_config(config)?);
        Self::new(config_service, RuntimeConfig::in_memory("admin-token")).await
    }

    pub fn validate_model_name(&self, requested: &str) -> crate::error::AppResult<String> {
        let snapshot = self.config_service.snapshot();
        if !snapshot.has_endpoint(requested, None) {
            return Err(crate::error::AppError::ModelNotFound(requested.to_string()));
        }

        Ok(requested.to_string())
    }

    pub fn validate_model_name_for_key(
        &self,
        requested: &str,
        key: &crate::config::AuthKey,
    ) -> crate::error::AppResult<String> {
        let snapshot = self.config_service.snapshot();
        if snapshot.has_endpoint(requested, Some(key)) {
            return Ok(requested.to_string());
        }
        Err(crate::error::AppError::ModelNotFound(requested.to_string()))
    }
}

/// Periodically closes stale running logs using the current total request timeout.
pub fn spawn_request_log_timeout_monitor(state: Arc<AppState>) -> tokio::task::JoinHandle<()> {
    let store = state.store.clone();
    let config_service = state.config_service.clone();

    tokio::spawn(async move {
        loop {
            let timeout =
                Duration::from_secs(config_service.snapshot().config.upstream.timeout_secs);
            match store.interrupt_timed_out_request_logs(timeout).await {
                Ok(count) if count > 0 => tracing::warn!(
                    request_count = count,
                    timeout_secs = timeout.as_secs(),
                    "Marked timed-out request logs as interrupted"
                ),
                Ok(_) => {}
                Err(error) => tracing::error!(
                    error = %error,
                    "Failed to close timed-out request logs"
                ),
            }

            let check_interval = timeout
                .min(Duration::from_secs(30))
                .max(Duration::from_secs(1));
            tokio::time::sleep(check_interval).await;
        }
    })
}
