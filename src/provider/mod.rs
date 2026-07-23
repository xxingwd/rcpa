pub mod anthropic;
pub mod client;
pub mod openai;

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;

use crate::config::{AppConfig, ProviderProtocol};
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
    by_name: HashMap<String, HashMap<ProviderProtocol, Arc<dyn ProviderAdapter>>>,
}

impl ProviderRegistry {
    pub fn from_config(config: &AppConfig) -> anyhow::Result<Self> {
        let mut by_name = HashMap::new();

        for provider_config in &config.providers {
            if !provider_config.is_enabled() || !provider_config.has_enabled_endpoint() {
                continue;
            }

            let mut adapters = HashMap::new();
            for endpoint in &provider_config.endpoints {
                let provider: Arc<dyn ProviderAdapter> = match endpoint.protocol {
                    ProviderProtocol::Completions
                    | ProviderProtocol::Responses
                    | ProviderProtocol::Embeddings => Arc::new(openai::OpenAIProvider::new(
                        provider_config,
                        &endpoint.base_url,
                        &config.upstream,
                    )?),
                    ProviderProtocol::Messages => Arc::new(anthropic::AnthropicProvider::new(
                        provider_config,
                        &endpoint.base_url,
                        &config.upstream,
                    )?),
                };
                adapters.insert(endpoint.protocol, provider);
            }

            by_name.insert(provider_config.name.clone(), adapters);
        }

        Ok(Self { by_name })
    }

    pub fn get(
        &self,
        provider_name: &str,
        protocol: ProviderProtocol,
    ) -> Option<Arc<dyn ProviderAdapter>> {
        self.by_name
            .get(provider_name)
            .and_then(|providers| providers.get(&protocol))
            .cloned()
    }

    pub fn connection_count(&self, name: &str) -> usize {
        self.by_name
            .get(name)
            .map(|providers| {
                providers
                    .values()
                    .map(|provider| provider.connection_count())
                    .sum()
            })
            .unwrap_or(0)
    }

    pub fn record_connection(&self, provider_name: &str, protocol: ProviderProtocol) {
        if let Some(provider) = self
            .by_name
            .get(provider_name)
            .and_then(|providers| providers.get(&protocol))
        {
            provider.increment_connection_for_test();
        }
    }

    pub fn release_connection(&self, provider_name: &str, protocol: ProviderProtocol) {
        if let Some(provider) = self
            .by_name
            .get(provider_name)
            .and_then(|providers| providers.get(&protocol))
        {
            provider.decrement_connection_for_test();
        }
    }
}
