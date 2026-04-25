use std::net::SocketAddr;

use tokio::net::TcpListener;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use verdict_api::app::router;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let address: SocketAddr = "127.0.0.1:3000".parse()?;
    let listener = TcpListener::bind(address).await?;
    info!("verdict-api listening on {address}");

    axum::serve(listener, router()).await?;
    Ok(())
}
