use std::net::SocketAddr;

use tokio::net::TcpListener;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use verdict_api::app::router;
use verdict_api::db::{connect, run_migrations};
use verdict_api::state::{AnthropicConfig, AppState};

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
    let anthropic_api_key = std::env::var("ANTHROPIC_API_KEY").map_err(|error| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("ANTHROPIC_API_KEY must be set: {error}"),
        )
    })?;
    let anthropic_model = std::env::var("ANTHROPIC_MODEL")
        .unwrap_or_else(|_| AnthropicConfig::DEFAULT_MODEL.to_string());
    let http_client = reqwest::Client::builder().build()?;
    let pool = connect(&database_url).await?;
    run_migrations(&pool).await?;

    let app_state = AppState {
        pool,
        http_client,
        anthropic: AnthropicConfig {
            api_key: anthropic_api_key,
            model: anthropic_model,
        },
    };
    let listener = TcpListener::bind(address).await?;
    info!("verdict-api listening on {address}");

    axum::serve(listener, router(app_state)).await?;
    Ok(())
}
