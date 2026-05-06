use std::net::SocketAddr;

use tokio::net::TcpListener;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use verdict_api::app::router;
use verdict_api::db::{connect, run_migrations};
use verdict_api::state::AppState;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let port = std::env::var("PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(3000);
    let address = SocketAddr::from(([0, 0, 0, 0], port));
    let database_url = std::env::var("DATABASE_URL")?;
    let pool = connect(&database_url).await?;
    run_migrations(&pool).await?;

    let app_state = AppState { pool };
    let listener = TcpListener::bind(address).await?;
    info!("verdict-api listening on {address}");

    axum::serve(listener, router(app_state)).await?;
    Ok(())
}
