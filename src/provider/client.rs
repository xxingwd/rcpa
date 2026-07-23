use crate::config::UpstreamConfig;
use reqwest::Client;
use std::time::Duration;

/// Build a shared HTTP client with connection pooling
pub fn build_client(config: &UpstreamConfig) -> anyhow::Result<Client> {
    let client = Client::builder()
        .pool_idle_timeout(Duration::from_secs(90))
        .timeout(Duration::from_secs(config.timeout_secs))
        .tcp_keepalive(Duration::from_secs(60))
        .http2_adaptive_window(true)
        .build()?;

    Ok(client)
}
