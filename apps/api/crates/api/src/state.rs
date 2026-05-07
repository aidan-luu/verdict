use sqlx::PgPool;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub http_client: reqwest::Client,
    pub anthropic: AnthropicConfig,
}

#[derive(Clone)]
pub struct AnthropicConfig {
    pub api_key: String,
    pub model: String,
}

impl AnthropicConfig {
    pub const DEFAULT_MODEL: &'static str = "claude-sonnet-4-6";
}

impl AppState {
    pub fn for_tests(pool: PgPool) -> Self {
        Self {
            pool,
            http_client: reqwest::Client::new(),
            anthropic: AnthropicConfig {
                api_key: "test-api-key".to_string(),
                model: AnthropicConfig::DEFAULT_MODEL.to_string(),
            },
        }
    }
}
