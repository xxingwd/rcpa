use std::sync::atomic::{AtomicUsize, Ordering};

use reqwest::Client;

use crate::config::{ProviderConfig, UpstreamConfig};
use crate::error::{AppError, AppResult};
use crate::protocol::common::{ProxyRequest, TokenUsage};

use super::client::build_client;
use super::{ProviderAdapter, ProviderResponse, ProviderStreamResponse};

/// OpenAI-compatible provider
pub struct OpenAIProvider {
    name: String,
    endpoint_url: String,
    client: Client,
    in_flight: AtomicUsize,
    headers: Vec<(String, String)>,
    /// Configured timeout in seconds, used for error reporting
    timeout_secs: u64,
}

impl OpenAIProvider {
    pub fn new(
        config: &ProviderConfig,
        endpoint_url: &str,
        upstream: &UpstreamConfig,
    ) -> anyhow::Result<Self> {
        let client = build_client(upstream)?;
        let mut headers: Vec<(String, String)> = vec![
            ("Authorization".into(), format!("Bearer {}", config.api_key)),
            ("Content-Type".into(), "application/json".into()),
        ];
        for (k, v) in &config.headers {
            headers.push((k.clone(), v.clone()));
        }

        Ok(Self {
            name: config.name.clone(),
            endpoint_url: endpoint_url.trim().to_string(),
            client,
            in_flight: AtomicUsize::new(0),
            headers,
            timeout_secs: upstream.timeout_secs,
        })
    }

    fn extract_tokens(body: &serde_json::Value) -> Option<TokenUsage> {
        TokenUsage::from_openai_body(body)
    }

    fn build_header_map(&self) -> reqwest::header::HeaderMap {
        let map = self
            .headers
            .iter()
            .fold(reqwest::header::HeaderMap::new(), |mut map, (k, v)| {
                if let (Ok(key), Ok(val)) = (
                    reqwest::header::HeaderName::from_bytes(k.as_bytes()),
                    reqwest::header::HeaderValue::from_str(v),
                ) {
                    map.insert(key, val);
                }
                map
            });
        map
    }

    fn resolve_url(&self, operation: &crate::protocol::common::Operation) -> AppResult<String> {
        match operation {
            crate::protocol::common::Operation::Completions
            | crate::protocol::common::Operation::Responses
            | crate::protocol::common::Operation::Embeddings => Ok(self.endpoint_url.clone()),
            _ => Err(AppError::ProtocolError(format!(
                "Unsupported operation {:?} for OpenAI provider",
                operation
            ))),
        }
    }

    fn rewrite_model_in_body(&self, req: &ProxyRequest) -> serde_json::Value {
        let mut body = req.body.clone();
        body["model"] = serde_json::Value::String(req.model.clone());
        if req.stream
            && matches!(
                req.operation,
                crate::protocol::common::Operation::Completions
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
    fn serialize_request_body(&self, req: &ProxyRequest) -> serde_json::Value {
        self.rewrite_model_in_body(req)
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
                    status_code: None,
                    error_code: None,
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
                        status_code: None,
                        error_code: None,
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
                if status >= 400 {
                    let text = resp.text().await.map_err(|e| AppError::ProviderError {
                        provider_name: self.name.clone(),
                        status_code: Some(
                            reqwest::StatusCode::from_u16(status)
                                .unwrap_or(reqwest::StatusCode::BAD_GATEWAY),
                        ),
                        error_code: None,
                        message: e.to_string(),
                    })?;
                    let body: serde_json::Value =
                        serde_json::from_str(&text).unwrap_or_else(|_| {
                            serde_json::json!({
                                "error": {
                                    "message": text,
                                    "type": "non_json_response"
                                }
                            })
                        });
                    let (error_code, error_message) =
                        crate::protocol::audit::extract_provider_error(&body);
                    return Err(AppError::ProviderError {
                        provider_name: self.name.clone(),
                        status_code: reqwest::StatusCode::from_u16(status).ok(),
                        error_code,
                        message: error_message
                            .unwrap_or_else(|| format!("Provider returned {}", status)),
                    });
                }
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
                        status_code: None,
                        error_code: None,
                        message: e.to_string(),
                    })
                }
            }
        }
    }

    fn connection_count(&self) -> usize {
        self.in_flight.load(Ordering::Relaxed)
    }

    fn increment_connection_for_test(&self) {
        self.in_flight.fetch_add(1, Ordering::Relaxed);
    }

    fn decrement_connection_for_test(&self) {
        self.in_flight.fetch_sub(1, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::OpenAIProvider;
    use crate::config::{
        EndpointConfig, ModelRule, ProviderConfig, ProviderProtocol, UpstreamConfig,
    };
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
        let provider_config = ProviderConfig {
            name: "openai".to_string(),
            api_key: "sk-test".to_string(),
            models: vec![ModelRule::enabled("gpt-4o")],
            endpoints: vec![EndpointConfig {
                protocol: ProviderProtocol::Completions,
                base_url: "https://api.openai.com/v1/chat/completions".to_string(),
            }],
            headers: HashMap::new(),
            priority: 1,
            status: "enabled".to_string(),
        };
        let provider = OpenAIProvider::new(
            &provider_config,
            "https://api.openai.com/v1/chat/completions",
            &UpstreamConfig { timeout_secs: 30 },
        )
        .unwrap();

        let req = ProxyRequest {
            id: Uuid::new_v4(),
            protocol: Protocol::Completions,
            operation: Operation::Completions,
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

    #[test]
    fn does_not_set_host_header_explicitly() {
        let provider_config = ProviderConfig {
            name: "openai".to_string(),
            api_key: "sk-test".to_string(),
            models: vec![ModelRule::enabled("gpt-4o")],
            endpoints: vec![EndpointConfig {
                protocol: ProviderProtocol::Completions,
                base_url: "https://api.openai.com/v1/chat/completions".to_string(),
            }],
            headers: HashMap::new(),
            priority: 1,
            status: "enabled".to_string(),
        };
        let provider = OpenAIProvider::new(
            &provider_config,
            "https://api.openai.com/v1/chat/completions",
            &UpstreamConfig { timeout_secs: 30 },
        )
        .unwrap();

        assert!(!provider
            .build_header_map()
            .contains_key(reqwest::header::HOST));
    }
}
