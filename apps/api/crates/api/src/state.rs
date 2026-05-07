use sqlx::PgPool;

use crate::ingest::pdf_fetch::PdfFetchConfig;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub http_client: reqwest::Client,
    pub gemini: GeminiConfig,
    pub pdf_fetch: PdfFetchConfig,
}

#[derive(Clone)]
pub struct GeminiConfig {
    pub api_key: String,
    pub model: String,
}

impl GeminiConfig {
    pub const DEFAULT_MODEL: &'static str = "gemini-2.0-flash";
}

impl AppState {
    pub fn for_tests(pool: PgPool) -> Self {
        Self {
            pool,
            http_client: reqwest::Client::new(),
            gemini: GeminiConfig {
                api_key: "test-api-key".to_string(),
                model: GeminiConfig::DEFAULT_MODEL.to_string(),
            },
            pdf_fetch: PdfFetchConfig::for_tests(),
        }
    }
}
