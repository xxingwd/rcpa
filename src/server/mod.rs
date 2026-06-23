pub mod router;

use std::sync::Arc;

use crate::config::AppConfig;
use crate::config_service::ConfigService;
use crate::routing::sticky::StickySessions;
use crate::routing::ModelRouter;
use crate::stats::StatsCollector;

/// Shared application state accessible from all handlers.
pub struct AppState {
    pub config_service: Arc<ConfigService>,
    pub router: Arc<ModelRouter>,
    pub stats: Arc<StatsCollector>,
    pub sticky_sessions: Arc<StickySessions>,
    pub store: crate::store::Store,
    pub start_time: chrono::DateTime<chrono::Utc>,
}

impl AppState {
    pub async fn new(config_service: Arc<ConfigService>) -> anyhow::Result<Self> {
        let snapshot = config_service.snapshot();
        let config = &snapshot.config;
        let store = crate::store::Store::open(&config.database.path).await?;
        let router = ModelRouter::new(config)?;
        let stats = StatsCollector::new();
        let sticky_sessions = StickySessions::new(config.routing.sticky.ttl_secs);

        Ok(Self {
            config_service,
            router: Arc::new(router),
            stats: Arc::new(stats),
            sticky_sessions: Arc::new(sticky_sessions),
            store,
            start_time: chrono::Utc::now(),
        })
    }

    pub async fn from_config(config: AppConfig) -> anyhow::Result<Self> {
        let config_service = Arc::new(ConfigService::from_config(config)?);
        Self::new(config_service).await
    }

    pub fn validate_model_name(&self, requested: &str) -> crate::error::AppResult<String> {
        let snapshot = self.config_service.snapshot();
        if !snapshot.has_model(requested) {
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
        if let Some(target) = snapshot.resolve_key_alias(requested, key) {
            if snapshot.has_model(&target) {
                return Ok(target);
            }
            return Err(crate::error::AppError::ModelNotFound(requested.to_string()));
        }

        self.validate_model_name(requested)
    }
}
