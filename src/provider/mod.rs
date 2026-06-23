pub mod anthropic;
pub mod client;
pub mod openai;

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;

use crate::config::{AppConfig, ProviderAdapterKind};
use crate::error::AppResult;
use crate::protocol::common::ProxyRequest;

/// A proxy response from a provider
#[derive(Debug, Clone)]
pub struct ProviderResponse {
    pub status: u16,
    pub body: serde_json::Value,
    pub tokens: Option<crate::protocol::common::TokenUsage>,
    pub first_byte_latency_ms: u64,
}

/// A streaming response from a provider
pub struct ProviderStreamResponse {
    pub status: u16,
    pub first_byte_latency_ms: u64,
    pub stream: Pin<Box<dyn futures::Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send>>,
}

/// Provider adapter abstraction trait
#[async_trait::async_trait]
pub trait ProviderAdapter: Send + Sync {
    /// Return the provider model name used for the outbound request.
    fn resolve_model(&self, model: &str) -> String;
    /// Serialize the exact outbound request JSON for this provider.
    fn serialize_request_body(&self, req: &ProxyRequest) -> serde_json::Value;

    async fn proxy(&self, req: ProxyRequest) -> AppResult<ProviderResponse>;
    /// Proxy a streaming request, returning a byte stream for SSE forwarding
    async fn proxy_stream(&self, req: ProxyRequest) -> AppResult<ProviderStreamResponse>;
    fn connection_count(&self) -> usize;
    fn increment_connection_for_test(&self) {}
    fn decrement_connection_for_test(&self) {}
}

/// Registry of all configured providers
pub struct ProviderRegistry {
    by_name: HashMap<String, Arc<dyn ProviderAdapter>>,
}

impl ProviderRegistry {
    pub fn from_config(config: &AppConfig) -> anyhow::Result<Self> {
        let mut by_name = HashMap::new();

        for provider_config in &config.providers {
            if !provider_config.is_enabled() {
                tracing::info!("Skipping disabled provider: {}", provider_config.name);
                continue;
            }

            let provider: Arc<dyn ProviderAdapter> = match provider_config.adapter {
                ProviderAdapterKind::Openai => {
                    Arc::new(openai::OpenAIProvider::new(provider_config)?)
                }
                ProviderAdapterKind::Anthropic => {
                    Arc::new(anthropic::AnthropicProvider::new(provider_config)?)
                }
            };

            by_name.insert(provider_config.name.clone(), provider.clone());
        }

        Ok(Self { by_name })
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn ProviderAdapter>> {
        self.by_name.get(name).cloned()
    }

    pub fn connection_count(&self, name: &str) -> usize {
        self.by_name
            .get(name)
            .map(|provider| provider.connection_count())
            .unwrap_or(0)
    }

    pub fn record_connection(&self, name: &str) {
        if let Some(provider) = self.by_name.get(name) {
            provider.increment_connection_for_test();
        }
    }

    pub fn release_connection(&self, name: &str) {
        if let Some(provider) = self.by_name.get(name) {
            provider.decrement_connection_for_test();
        }
    }
}
