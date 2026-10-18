use std::net::SocketAddr;

use protolive_server::server::{AppState, ServerConfig, router};
use tokio::net::TcpListener;
use tokio::sync::watch;
use tracing::info;
use tracing_subscriber::EnvFilter;

const DEFAULT_ADDR: &str = "127.0.0.1:3000";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let addr: SocketAddr = std::env::var("PROTOLIVE_ADDR")
        .unwrap_or_else(|_| DEFAULT_ADDR.to_owned())
        .parse()?;
    let listener = TcpListener::bind(addr).await?;
    info!(addr = %listener.local_addr()?, "待ち受け開始 (ws://…/ws)");

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let app = router(AppState::new(ServerConfig::default(), shutdown_rx));
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let _ = tokio::signal::ctrl_c().await;
            info!("シャットダウン開始");
            // 接続中の WebSocket にも終了を通知しないと serve が終わらない。
            let _ = shutdown_tx.send(true);
        })
        .await?;
    Ok(())
}
