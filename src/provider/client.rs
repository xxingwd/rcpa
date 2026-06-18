use reqwest::Client;
use std::time::Duration;

/// Build a shared HTTP client with connection pooling
pub fn build_client(max_connections: usize, timeout_secs: u64) -> anyhow::Result<Client> {
    let client = Client::builder()
        .pool_max_idle_per_host(max_connections)
        .pool_idle_timeout(Duration::from_secs(90))
        .timeout(Duration::from_secs(timeout_secs))
        .tcp_keepalive(Duration::from_secs(60))
        .http2_adaptive_window(true)
        .build()?;

    Ok(client)
}
