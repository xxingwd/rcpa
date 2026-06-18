use std::sync::atomic::{AtomicUsize, Ordering};

use reqwest::Client;

use crate::config::ProviderConfig;
use crate::error::{AppError, AppResult};
use crate::protocol::common::{ProxyRequest, TokenUsage};

use super::client::build_client;
use super::{ProviderAdapter, ProviderResponse, ProviderStreamResponse};

/// Anthropic provider
pub struct AnthropicProvider {
    name: String,
    protocol: String,
    base_url: String,
    models: Vec<String>,
    model_mappings: std::collections::HashMap<String, String>,
    pricing: std::collections::HashMap<String, (f64, f64)>,
    client: Client,
    in_flight: AtomicUsize,
    headers: Vec<(String, String)>,
    /// Configured timeout in seconds, used for error reporting
    timeout_secs: u64,
}

impl AnthropicProvider {
    pub fn new(config: &ProviderConfig) -> anyhow::Result<Self> {
        let client = build_client(config.max_connections, config.timeout_secs)?;
        let mut headers: Vec<(String, String)> = vec![
            ("x-api-key".into(), config.api_key.clone()),
            ("anthropic-version".into(), "2023-06-01".into()),
            ("Content-Type".into(), "application/json".into()),
        ];
        for (k, v) in &config.headers {
            headers.push((k.clone(), v.clone()));
        }

        Ok(Self {
            name: config.name.clone(),
            protocol: config.protocol.clone(),
            base_url: config.base_url.trim_end_matches('/').to_string(),
            models: config.enabled_model_names(),
            model_mappings: config.enabled_model_mappings(),
            pricing: config
                .enabled_pricing()
                .into_iter()
                .map(|(k, v)| (k.clone(), (v.input_per_1k, v.output_per_1k)))
                .collect(),
            client,
            in_flight: AtomicUsize::new(0),
            headers,
            timeout_secs: config.timeout_secs,
        })
    }

    fn extract_tokens(body: &serde_json::Value) -> Option<TokenUsage> {
        TokenUsage::from_anthropic_body(body)
    }

    fn build_header_map(&self) -> reqwest::header::HeaderMap {
        self.headers
            .iter()
            .fold(reqwest::header::HeaderMap::new(), |mut map, (k, v)| {
                if let (Ok(key), Ok(val)) = (
                    reqwest::header::HeaderName::from_bytes(k.as_bytes()),
                    reqwest::header::HeaderValue::from_str(v),
                ) {
                    map.insert(key, val);
                }
                map
            })
    }

    /// Rewrite the outbound body with the resolved provider model.
    /// Anthropic always requires the "model" field.
    fn rewrite_model_in_body(&self, req: &ProxyRequest) -> serde_json::Value {
        let mut body = req.body.clone();
        let resolved = self.resolve_model(&req.model);
        body["model"] = serde_json::Value::String(resolved);
        body
    }
}

#[async_trait::async_trait]
impl ProviderAdapter for AnthropicProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn protocol(&self) -> &str {
        &self.protocol
    }

    fn models(&self) -> &[String] {
        &self.models
    }

    fn base_url(&self) -> &str {
        &self.base_url
    }

    fn resolve_model(&self, model: &str) -> String {
        self.model_mappings
            .get(model)
            .cloned()
            .unwrap_or_else(|| model.to_string())
    }

    fn serialize_request_body(&self, req: &ProxyRequest) -> serde_json::Value {
        self.rewrite_model_in_body(req)
    }

    fn model_pricing(&self, model: &str) -> Option<(f64, f64)> {
        self.pricing.get(model).copied()
    }

    async fn proxy(&self, req: ProxyRequest) -> AppResult<ProviderResponse> {
        let url = format!("{}/v1/messages", self.base_url);
        let body = self.rewrite_model_in_body(&req);
        let start = std::time::Instant::now();

        self.in_flight.fetch_add(1, Ordering::Relaxed);
        let result = self
            .client
            .post(&url)
            .headers(self.build_header_map())
            .json(&body)
            .send()
            .await;
        self.in_flight.fetch_sub(1, Ordering::Relaxed);

        match result {
            Ok(resp) => {
                let first_byte_latency_ms = start.elapsed().as_millis() as u64;
                let status = resp.status().as_u16();
                let text = resp.text().await.map_err(|e| AppError::ProviderError {
                    provider_name: self.name.clone(),
                    message: e.to_string(),
                })?;

                let body: serde_json::Value = serde_json::from_str(&text).unwrap_or_else(|_| {
                    serde_json::json!({ "error": { "message": text, "type": "non_json_response" } })
                });

                let tokens = Self::extract_tokens(&body);

                Ok(ProviderResponse {
                    status,
                    body,
                    tokens,
                    first_byte_latency_ms,
                })
            }
            Err(e) => {
                if e.is_timeout() {
                    Err(AppError::ProviderTimeout(std::time::Duration::from_secs(
                        self.timeout_secs,
                    )))
                } else {
                    Err(AppError::ProviderError {
                        provider_name: self.name.clone(),
                        message: e.to_string(),
                    })
                }
            }
        }
    }

    async fn proxy_stream(&self, req: ProxyRequest) -> AppResult<ProviderStreamResponse> {
        let url = format!("{}/v1/messages", self.base_url);
        let body = self.rewrite_model_in_body(&req);
        let start = std::time::Instant::now();

        self.in_flight.fetch_add(1, Ordering::Relaxed);
        let result = self
            .client
            .post(&url)
            .headers(self.build_header_map())
            .json(&body)
            .send()
            .await;
        self.in_flight.fetch_sub(1, Ordering::Relaxed);

        match result {
            Ok(resp) => {
                let first_byte_latency_ms = start.elapsed().as_millis() as u64;
                let status = resp.status().as_u16();
                let stream = resp.bytes_stream();
                Ok(ProviderStreamResponse {
                    status,
                    first_byte_latency_ms,
                    stream: Box::pin(stream),
                })
            }
            Err(e) => {
                if e.is_timeout() {
                    Err(AppError::ProviderTimeout(std::time::Duration::from_secs(
                        self.timeout_secs,
                    )))
                } else {
                    Err(AppError::ProviderError {
                        provider_name: self.name.clone(),
                        message: e.to_string(),
                    })
                }
            }
        }
    }

    fn connection_count(&self) -> usize {
        self.in_flight.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::AnthropicProvider;

    #[test]
    fn extracts_anthropic_usage_with_cache_tokens() {
        let body = serde_json::json!({
            "usage": {
                "input_tokens": 20,
                "output_tokens": 8,
                "cache_read_input_tokens": 9,
                "cache_creation_input_tokens": 4
            }
        });

        let tokens = AnthropicProvider::extract_tokens(&body).unwrap();
        assert_eq!(tokens.prompt_tokens, 33);
        assert_eq!(tokens.completion_tokens, 8);
        assert_eq!(tokens.total_tokens, 41);
        assert_eq!(tokens.cached_tokens, 9);
        assert_eq!(tokens.cache_write_tokens, 4);
    }
}
