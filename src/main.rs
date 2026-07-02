use std::{future::IntoFuture, path::PathBuf, sync::Arc};

use arc_swap::ArcSwap;
use clap::{Parser, Subcommand};
use claude_code_proxy::{
    build_app, build_dashboard_routes, AppConfig, DashboardState, Runtime, SharedState,
};
use claude_code_proxy::error::ErrorKind;
use metrics_exporter_prometheus::PrometheusBuilder;
use tokio::net::TcpListener;

#[derive(Parser)]
#[command(
    name = "claude-code-proxy",
    version,
    about = "Production Anthropic-to-OpenAI gateway with dashboard"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Serve {
        #[arg(short, long, default_value = "config.yaml")]
        config: PathBuf,
        #[arg(long, default_value = "proxy.db")]
        database: PathBuf,
        #[arg(long, env = "PROXY_ADMIN_SECRET", default_value = "")]
        admin_secret: String,
    },
    CheckConfig {
        #[arg(short, long, default_value = "config.yaml")]
        config: PathBuf,
    },
    PrintEffectiveConfig {
        #[arg(short, long, default_value = "config.yaml")]
        config: PathBuf,
    },
    Probe {
        #[arg(short, long, default_value = "config.yaml")]
        config: PathBuf,
        #[arg(long)]
        provider: String,
    },
}

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

async fn run() -> claude_code_proxy::error::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Serve {
            config,
            database,
            admin_secret,
        } =>
            serve(config, database, admin_secret).await,
        Command::CheckConfig { config } => {
            Runtime::new(AppConfig::load(config)?)?;
            println!("configuration is valid");
            Ok(())
        }
        Command::PrintEffectiveConfig { config } => {
            let config = AppConfig::load(config)?;
            println!("bind: {}", config.server.bind);
            println!("clients: {}", config.clients.len());
            println!(
                "providers: {}",
                config
                    .providers
                    .iter()
                    .map(|provider| provider.id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            println!(
                "routes: {}",
                config
                    .routes
                    .iter()
                    .map(|route| route.id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            println!("secret values: [redacted]");
            Ok(())
        }
        Command::Probe { config, provider } => {
            let config = AppConfig::load(config)?;
            let runtime = Runtime::new(config)?;
            runtime.router.probe(&provider).await?;
            println!("provider {provider} is reachable and authenticated");
            Ok(())
        }
    }
}

async fn serve(
    path: PathBuf,
    database_path: PathBuf,
    admin_secret: String,
) -> claude_code_proxy::error::Result<()> {
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,tower_http=info".into()),
        )
        .init();
    let config = AppConfig::load(&path)?;
    let bind = config.server.bind;
    let metrics_bind = config.server.metrics_bind;
    let shutdown_grace = std::time::Duration::from_secs(config.server.shutdown_grace_seconds);
    let runtime = Arc::new(Runtime::new(config)?);
    let shared: SharedState = Arc::new(ArcSwap::from(runtime));
    spawn_reload(path, shared.clone());

    // Initialize dashboard state
    let database_url = format!("sqlite:{}", database_path.display());
    let db_pool = claude_code_proxy::create_db_pool(&database_url).await?;
    claude_code_proxy::initialize_database(&db_pool).await?;
    claude_code_proxy::create_initial_admin_user(&db_pool, &admin_secret).await?;
    claude_code_proxy::create_sample_config(&db_pool).await?;
    claude_code_proxy::create_sample_client(&db_pool).await?;
    claude_code_proxy::create_sample_provider(&db_pool).await?;
    claude_code_proxy::create_sample_route(&db_pool).await?;
    let dashboard_state = Arc::new(DashboardState {
        db_pool,
        admin_secret,
    });

    if let Some(metrics_bind) = metrics_bind {
        let handle = PrometheusBuilder::new()
            .install_recorder()
            .map_err(|error| {
                claude_code_proxy::error::ProxyError::new(
                    ErrorKind::Internal,
                    format!("metrics initialization failed: {error}"),
                )
            })?;
        tokio::spawn(async move {
            let app = axum::Router::new().route(
                "/metrics",
                axum::routing::get(move || {
                    let handle = handle.clone();
                    async move { handle.render() }
                }),
            );
            match TcpListener::bind(metrics_bind).await {
                Ok(listener) => {
                    if let Err(error) = axum::serve(listener, app).await {
                        tracing::error!(%error, "metrics server stopped");
                    }
                }
                Err(error) => tracing::error!(%error, "cannot bind metrics listener"),
            }
        });
    }

    let app = build_app(shared);
    let dashboard_app = build_dashboard_routes(dashboard_state);

    // Combine both apps
    let combined_app = app.merge(dashboard_app);

    let listener = TcpListener::bind(bind).await.map_err(|error| {
        claude_code_proxy::error::ProxyError::new(
            ErrorKind::Internal,
            format!("cannot bind {bind}: {error}"),
        )
    })?;
    tracing::info!(%bind, "proxy listening");
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let server = axum::serve(listener, combined_app)
        .with_graceful_shutdown(async move {
            let _ = shutdown_rx.await;
        })
        .into_future();
    tokio::pin!(server);
    tokio::select! {
        result = &mut server => result.map_err(|error| claude_code_proxy::error::ProxyError::new(ErrorKind::Internal, format!("server failed: {error}")))?,
        _ = shutdown_signal() => {
            let _ = shutdown_tx.send(());
            if tokio::time::timeout(shutdown_grace, &mut server).await.is_err() {
                tracing::warn!(?shutdown_grace, "shutdown grace expired; cancelling remaining requests");
            }
        }
    }
    Ok(())
}

#[cfg(unix)]
fn spawn_reload(path: PathBuf, shared: SharedState) {
    tokio::spawn(async move {
        let mut signal = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup())
            .expect("SIGHUP handler");
        while signal.recv().await.is_some() {
            match AppConfig::load(&path).and_then(Runtime::new) {
                Ok(runtime) if runtime.config.server == shared.load().config.server => {
                    shared.store(Arc::new(runtime));
                    metrics::counter!("proxy_config_reload_total", "outcome" => "success")
                        .increment(1);
                    tracing::info!("configuration reloaded");
                }
                Ok(_) => {
                    metrics::counter!("proxy_config_reload_total", "outcome" => "rejected")
                        .increment(1);
                    tracing::error!("configuration reload rejected: server listener and body-limit settings require restart");
                }
                Err(error) => {
                    metrics::counter!("proxy_config_reload_total", "outcome" => "rejected")
                        .increment(1);
                    tracing::error!(%error, "configuration reload rejected; retaining previous runtime");
                }
            }
        }
    });
}

#[cfg(not(unix))]
fn spawn_reload(_path: PathBuf, _shared: SharedState) {}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("SIGTERM handler");
        tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = terminate.recv() => {} }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
    tracing::info!("shutdown requested");
}
