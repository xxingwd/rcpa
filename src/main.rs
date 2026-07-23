use clap::Parser;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use tokio::signal;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use rcpa::config::{
    config_path_for_data_dir, log_dir_for_data_dir, sqlite_path_for_data_dir, AppConfig,
};
use rcpa::config_service::ConfigService;
use rcpa::server::{self, RuntimeConfig};

/// RCPA - Rust Cloud Proxy API
/// High-performance LLM API gateway supporting multiple AI providers
#[derive(Parser, Debug)]
#[command(name = "rcpa", version, about, long_about = None)]
struct Cli {
    /// Runtime data directory. RCPA uses fixed names under this directory.
    #[arg(short, long, default_value = "~/.rcpa")]
    data_dir: String,

    /// Admin UI/API token. Must be provided explicitly at process start.
    #[arg(long)]
    token: String,

    /// HTTP listen port
    #[arg(short, long, default_value_t = 15000)]
    port: u16,

    /// Log level (trace, debug, info, warn, error)
    #[arg(short, long, default_value = "info")]
    log_level: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    if cli.token.trim().is_empty() {
        anyhow::bail!("--token cannot be empty");
    }

    let data_dir = rcpa::config::expand_tilde(Path::new(&cli.data_dir));
    let config_path = config_path_for_data_dir(&data_dir);
    let sqlite_path = sqlite_path_for_data_dir(&data_dir);
    let log_dir = log_dir_for_data_dir(&data_dir);

    let bootstrap = AppConfig::ensure_config_file(&config_path)?;
    std::fs::create_dir_all(&log_dir)?;

    // Initialize tracing
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&cli.log_level));
    let file_appender = tracing_appender::rolling::daily(&log_dir, "rcpa.log");
    let (file_writer, _log_guard) = tracing_appender::non_blocking(file_appender);
    let local_timer = tracing_subscriber::fmt::time::ChronoLocal::rfc_3339();

    tracing_subscriber::registry()
        .with(env_filter)
        .with(
            tracing_subscriber::fmt::layer()
                .with_timer(local_timer.clone())
                .with_target(true)
                .with_thread_ids(true)
                .with_line_number(true),
        )
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(file_writer)
                .with_timer(local_timer)
                .with_ansi(false)
                .with_target(true)
                .with_thread_ids(true)
                .with_line_number(true),
        )
        .init();

    if let Some(info) = bootstrap {
        tracing::info!(
            config = %info.path.display(),
            data_dir = %info.data_dir.display(),
            sqlite = %info.sqlite_path.display(),
            logs = %info.log_dir.display(),
            "Created bootstrap config"
        );
    }

    tracing::info!("RCPA starting up...");
    tracing::info!(
        timezone = %std::env::var("TZ").unwrap_or_else(|_| "system-default".to_string()),
        local_time = %chrono::Local::now().to_rfc3339(),
        "Runtime timezone initialized"
    );

    // Load configuration
    let config = AppConfig::load(config_path.to_str().ok_or_else(|| {
        anyhow::anyhow!("Config path '{}' is not valid UTF-8", config_path.display())
    })?)?;
    tracing::info!("Loaded config: {} providers", config.providers.len(),);

    // Build application state
    let config_service = Arc::new(ConfigService::new(&config_path)?);
    let state = Arc::new(
        server::AppState::new(
            config_service,
            RuntimeConfig::new(cli.token.clone(), sqlite_path.clone()),
        )
        .await?,
    );
    let _request_log_body_gc = rcpa::store::spawn_request_log_body_gc(state.store.clone());

    // Build router
    let app = server::router::build(state.clone());

    // Bind and serve
    let addr = SocketAddr::new("0.0.0.0".parse()?, cli.port);

    tracing::info!("RCPA listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    tracing::info!("RCPA shutting down");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            tracing::info!("Received Ctrl+C, shutting down...");
        },
        _ = terminate => {
            tracing::info!("Received SIGTERM, shutting down...");
        },
    }
}
