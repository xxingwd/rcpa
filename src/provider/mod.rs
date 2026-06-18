pub mod anthropic;
pub mod client;
pub mod openai;

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;

use crate::config::AppConfig;
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
    fn name(&self) -> &str;
    fn protocol(&self) -> &str;
    fn models(&self) -> &[String];
    fn base_url(&self) -> &str;
    /// Return the provider model name used for the outbound request.
    fn resolve_model(&self, model: &str) -> String;
    /// Serialize the exact outbound request JSON for this provider.
    fn serialize_request_body(&self, req: &ProxyRequest) -> serde_json::Value;
    /// Get per-model pricing for this provider (if configured)
    fn model_pricing(&self, model: &str) -> Option<(f64, f64)>;

    async fn proxy(&self, req: ProxyRequest) -> AppResult<ProviderResponse>;
    /// Proxy a streaming request, returning a byte stream for SSE forwarding
    async fn proxy_stream(&self, req: ProxyRequest) -> AppResult<ProviderStreamResponse>;
    fn connection_count(&self) -> usize;
}

/// Registry of all configured providers
pub struct ProviderRegistry {
    providers: Vec<Arc<dyn ProviderAdapter>>,
    by_name: HashMap<String, Arc<dyn ProviderAdapter>>,
}

impl ProviderRegistry {
    pub fn from_config(config: &AppConfig) -> anyhow::Result<Self> {
        let mut providers: Vec<Arc<dyn ProviderAdapter>> = Vec::new();
        let mut by_name = HashMap::new();

        for provider_config in &config.providers {
            if !provider_config.is_enabled() {
                tracing::info!("Skipping disabled provider: {}", provider_config.name);
                continue;
            }

            let provider: Arc<dyn ProviderAdapter> = match provider_config.protocol.as_str() {
                "completions" | "responses" => {
                    Arc::new(openai::OpenAIProvider::new(provider_config)?)
                }
                "messages" => Arc::new(anthropic::AnthropicProvider::new(provider_config)?),
                other => anyhow::bail!("Unknown provider protocol: {}", other),
            };

            by_name.insert(provider_config.name.clone(), provider.clone());
            providers.push(provider);
        }

        Ok(Self { providers, by_name })
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn ProviderAdapter>> {
        self.by_name.get(name).cloned()
    }

    pub fn len(&self) -> usize {
        self.providers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.providers.is_empty()
    }

    pub fn connection_count(&self, name: &str) -> usize {
        self.by_name
            .get(name)
            .map(|provider| provider.connection_count())
            .unwrap_or(0)
    }

    /// List all available models across all providers
    pub fn list_models(&self) -> Vec<(String, String)> {
        let mut models = Vec::new();
        for provider in &self.providers {
            for model in provider.models() {
                models.push((model.clone(), provider.name().to_string()));
            }
        }
        models
    }

    /// Check if any provider supports a given model name
    pub fn has_model(&self, model: &str) -> bool {
        self.providers
            .iter()
            .any(|provider| provider.models().iter().any(|m| m == model))
    }
}
