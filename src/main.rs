use clap::Parser;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::signal;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use rcpa::config::{data_dir_for_config, AppConfig};
use rcpa::config_service::ConfigService;
use rcpa::server;

/// RCPA - Rust Cloud Proxy API
/// High-performance LLM API gateway supporting multiple AI providers
#[derive(Parser, Debug)]
#[command(name = "rcpa", version, about, long_about = None)]
struct Cli {
    /// Path to configuration file
    #[arg(short, long, default_value = "~/.rcpa/config.yaml")]
    config: String,

    /// Log level (trace, debug, info, warn, error)
    #[arg(short, long, default_value = "info")]
    log_level: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let config_path = PathBuf::from(&cli.config);
    let bootstrap = AppConfig::ensure_config_file(&config_path)?;
    let data_dir = data_dir_for_config(&config_path);
    let log_dir = data_dir.join("logs");
    std::fs::create_dir_all(&log_dir)?;

    // Initialize tracing
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&cli.log_level));
    let file_appender = tracing_appender::rolling::daily(&log_dir, "rcpa.log");
    let (file_writer, _log_guard) = tracing_appender::non_blocking(file_appender);

    tracing_subscriber::registry()
        .with(env_filter)
        .with(
            tracing_subscriber::fmt::layer()
                .with_target(true)
                .with_thread_ids(true)
                .with_line_number(true),
        )
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(file_writer)
                .with_ansi(false)
                .with_target(true)
                .with_thread_ids(true)
                .with_line_number(true),
        )
        .init();

    if let Some(info) = bootstrap {
        tracing::info!(
            config = %info.path.display(),
            database = %info.database_path.display(),
            logs = %info.log_dir.display(),
            "Created bootstrap config"
        );
    }

    tracing::info!("RCPA starting up...");

    // Load configuration
    let config = AppConfig::load(&cli.config)?;
    tracing::info!("Loaded config: {} providers", config.providers.len(),);

    // Build application state
    let config_service = Arc::new(ConfigService::new(&cli.config)?);
    let state = Arc::new(server::AppState::new(config_service).await?);

    // Build router
    let app = server::router::build(state.clone());

    // Bind and serve
    let addr = SocketAddr::new(config.server.host.parse()?, config.server.port);

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
