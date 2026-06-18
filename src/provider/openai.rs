use std::sync::atomic::{AtomicUsize, Ordering};

use reqwest::Client;

use crate::config::ProviderConfig;
use crate::error::{AppError, AppResult};
use crate::protocol::common::{ProxyRequest, TokenUsage};

use super::client::build_client;
use super::{ProviderAdapter, ProviderResponse, ProviderStreamResponse};

/// OpenAI-compatible provider
pub struct OpenAIProvider {
    name: String,
    protocol: String,
    base_url: String,
    models: Vec<String>,
    model_mappings: std::collections::HashMap<String, String>,
    /// Per-model pricing overrides
    pricing: std::collections::HashMap<String, (f64, f64)>,
    client: Client,
    in_flight: AtomicUsize,
    headers: Vec<(String, String)>,
    api_version: Option<String>,
    /// Configured timeout in seconds, used for error reporting
    timeout_secs: u64,
}

impl OpenAIProvider {
    pub fn new(config: &ProviderConfig) -> anyhow::Result<Self> {
        let client = build_client(config.max_connections, config.timeout_secs)?;
        let mut headers: Vec<(String, String)> = vec![
            ("Authorization".into(), format!("Bearer {}", config.api_key)),
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
            api_version: config.api_version.clone(),
            timeout_secs: config.timeout_secs,
        })
    }

    fn build_url(&self, path: &str) -> String {
        if let Some(ref version) = self.api_version {
            format!("{}/{}?api-version={}", self.base_url, path, version)
        } else {
            format!("{}/{}", self.base_url, path)
        }
    }

    fn extract_tokens(body: &serde_json::Value) -> Option<TokenUsage> {
        TokenUsage::from_openai_body(body)
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

    fn resolve_url(&self, operation: &crate::protocol::common::Operation) -> AppResult<String> {
        match operation {
            crate::protocol::common::Operation::ChatCompletions => {
                Ok(self.build_url("v1/chat/completions"))
            }
            crate::protocol::common::Operation::Completions => Ok(self.build_url("v1/completions")),
            crate::protocol::common::Operation::Responses => Ok(self.build_url("v1/responses")),
            crate::protocol::common::Operation::Embeddings => Ok(self.build_url("v1/embeddings")),
            _ => Err(AppError::ProtocolError(format!(
                "Unsupported operation {:?} for OpenAI provider",
                operation
            ))),
        }
    }

    /// Rewrite the outbound body with the resolved provider model.
    fn rewrite_model_in_body(&self, req: &ProxyRequest) -> serde_json::Value {
        let mut body = req.body.clone();
        let resolved = self.resolve_model(&req.model);
        body["model"] = serde_json::Value::String(resolved);
        if req.stream
            && matches!(
                req.operation,
                crate::protocol::common::Operation::ChatCompletions
                    | crate::protocol::common::Operation::Completions
            )
        {
            body["stream_options"] = match body.get("stream_options") {
                Some(serde_json::Value::Object(existing)) => {
                    let mut options = existing.clone();
                    options.insert("include_usage".to_string(), serde_json::Value::Bool(true));
                    serde_json::Value::Object(options)
                }
                _ => serde_json::json!({ "include_usage": true }),
            };
        }
        body
    }
}

#[async_trait::async_trait]
impl ProviderAdapter for OpenAIProvider {
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
        let url = self.resolve_url(&req.operation)?;
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
        let url = self.resolve_url(&req.operation)?;
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
    use super::OpenAIProvider;
    use crate::config::{ModelRule, ProviderConfig};
    use crate::protocol::common::{Operation, Protocol, ProxyRequest};
    use crate::provider::ProviderAdapter;
    use std::collections::HashMap;
    use uuid::Uuid;

    #[test]
    fn extracts_chat_usage_with_cached_tokens() {
        let body = serde_json::json!({
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 4,
                "total_tokens": 14,
                "prompt_tokens_details": {
                    "cached_tokens": 6,
                    "cache_write_tokens": 2
                }
            }
        });

        let tokens = OpenAIProvider::extract_tokens(&body).unwrap();
        assert_eq!(tokens.prompt_tokens, 10);
        assert_eq!(tokens.completion_tokens, 4);
        assert_eq!(tokens.total_tokens, 14);
        assert_eq!(tokens.cached_tokens, 6);
        assert_eq!(tokens.cache_write_tokens, 2);
    }

    #[test]
    fn extracts_responses_usage_shape() {
        let body = serde_json::json!({
            "usage": {
                "input_tokens": 12,
                "output_tokens": 3,
                "input_tokens_details": {
                    "cached_tokens": 5
                }
            }
        });

        let tokens = OpenAIProvider::extract_tokens(&body).unwrap();
        assert_eq!(tokens.prompt_tokens, 12);
        assert_eq!(tokens.completion_tokens, 3);
        assert_eq!(tokens.total_tokens, 15);
        assert_eq!(tokens.cached_tokens, 5);
        assert_eq!(tokens.cache_write_tokens, 0);
    }

    #[test]
    fn streaming_chat_request_includes_usage_option() {
        let provider = OpenAIProvider::new(&ProviderConfig {
            name: "openai".to_string(),
            protocol: "completions".to_string(),
            base_url: "https://api.openai.com".to_string(),
            api_key: "sk-test".to_string(),
            models: vec![ModelRule::enabled("gpt-4o")],
            weight: 10,
            max_connections: 10,
            timeout_secs: 30,
            headers: HashMap::new(),
            api_version: None,
            status: "enabled".to_string(),
            priority: 0,
            group: "default".to_string(),
        })
        .unwrap();

        let req = ProxyRequest {
            id: Uuid::new_v4(),
            protocol: Protocol::Completions,
            operation: Operation::ChatCompletions,
            model: "gpt-4o".to_string(),
            body: serde_json::json!({
                "model": "gpt-4o",
                "stream": true,
                "messages": [{ "role": "user", "content": "hi" }]
            }),
            stream: true,
        };

        let body = provider.serialize_request_body(&req);
        assert_eq!(body["stream_options"]["include_usage"], true);
    }
}
