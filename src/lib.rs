pub mod admin;
pub mod http_client;
pub mod proxy;
pub mod response;
pub mod settings;
pub mod state;

use std::net::SocketAddr;
use std::sync::Arc;

use admin::router as admin_router;
use anyhow::{Context, anyhow};
use http_client::ReqwestHttpClient;
use proxy::router as proxy_router;
use settings::SettingsLayer;
use state::AppState;
use tracing::{error, info};

use axum::Router;
use tokio::net::TcpListener;

pub async fn run() -> anyhow::Result<()> {
    let config = server_config_from_env()?;
    let env_layer = SettingsLayer::from_env();
    let development_trailer = if std::env::var("MIKKMOKK_DEVELOPMENT")
        .map(|v| v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
    {
        "\n".to_string()
    } else {
        String::new()
    };

    let client =
        Arc::new(ReqwestHttpClient::new().context("failed to create outbound HTTP client")?);
    let state = Arc::new(AppState::new(env_layer, development_trailer, client));
    state.log_env_overrides();

    let proxy = proxy_router(state.clone());
    let admin = admin_router(state);

    run_servers(config, proxy, admin).await
}

struct ServerConfig {
    proxy_addr: SocketAddr,
    admin_addr: SocketAddr,
}

fn server_config_from_env() -> anyhow::Result<ServerConfig> {
    let proxy_addr = resolve_addr("PROXY_BIND", "PROXY_PORT", "127.0.0.1", 8080)
        .context("invalid proxy bind configuration")?;
    let admin_addr = resolve_addr("ADMIN_BIND", "ADMIN_PORT", "127.0.0.1", 7070)
        .context("invalid admin bind configuration")?;
    Ok(ServerConfig {
        proxy_addr,
        admin_addr,
    })
}

fn resolve_addr(
    bind_key: &str,
    port_key: &str,
    default_bind: &str,
    default_port: u16,
) -> anyhow::Result<SocketAddr> {
    let bind = std::env::var(bind_key).unwrap_or_else(|_| default_bind.to_string());
    let port = std::env::var(port_key)
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(default_port);
    let socket = format!("{bind}:{port}");
    socket
        .parse()
        .with_context(|| format!("could not parse address {socket}"))
}

async fn run_servers(
    config: ServerConfig,
    proxy_router: Router,
    admin_router: Router,
) -> anyhow::Result<()> {
    info!("Starting admin server at {}", config.admin_addr);
    info!("Starting proxy server at {}", config.proxy_addr);

    let proxy_listener = TcpListener::bind(config.proxy_addr)
        .await
        .context("failed to bind proxy listener")?;
    let admin_listener = TcpListener::bind(config.admin_addr)
        .await
        .context("failed to bind admin listener")?;

    let proxy_shutdown = shutdown_signal("proxy");
    let admin_shutdown = shutdown_signal("admin");

    let proxy_server = axum::serve(proxy_listener, proxy_router.into_make_service())
        .with_graceful_shutdown(proxy_shutdown);
    let admin_server = axum::serve(admin_listener, admin_router.into_make_service())
        .with_graceful_shutdown(admin_shutdown);

    tokio::try_join!(
        async {
            proxy_server.await.map_err(|err| {
                error!("proxy server exited with error: {err}");
                anyhow!("proxy server error: {err}")
            })
        },
        async {
            admin_server.await.map_err(|err| {
                error!("admin server exited with error: {err}");
                anyhow!("admin server error: {err}")
            })
        }
    )?;

    Ok(())
}

async fn shutdown_signal(component: &'static str) {
    if let Err(err) = tokio::signal::ctrl_c().await {
        error!("failed to install CTRL+C handler for {component}: {err}");
    }
    info!("Shutting down {component} server");
}
