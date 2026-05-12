use std::sync::Arc;

use sqlx::PgPool;

use crate::ingest::gemini_briefing::{BriefingExtractor, StubBriefingExtractor};
use crate::ingest::pdf_fetch::PdfFetchConfig;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub http_client: reqwest::Client,
    pub gemini: GeminiConfig,
    pub pdf_fetch: PdfFetchConfig,
    pub briefing: Arc<dyn BriefingExtractor + Send + Sync>,
}

#[derive(Clone)]
pub struct GeminiConfig {
    pub api_key: String,
    pub model: String,
}

impl GeminiConfig {
    /// Default for new API keys: `gemini-2.0-*` is deprecated for new users; Flash-Lite keeps cost low.
    pub const DEFAULT_MODEL: &'static str = "gemini-2.5-flash-lite";
}

impl AppState {
    pub fn for_tests(pool: PgPool) -> Self {
        Self::for_tests_with_briefing(pool, Arc::new(StubBriefingExtractor))
    }

    pub fn for_tests_with_briefing(
        pool: PgPool,
        briefing: Arc<dyn BriefingExtractor + Send + Sync>,
    ) -> Self {
        Self {
            pool,
            http_client: reqwest::Client::new(),
            gemini: GeminiConfig {
                api_key: "test-api-key".to_string(),
                model: GeminiConfig::DEFAULT_MODEL.to_string(),
            },
            pdf_fetch: PdfFetchConfig::for_tests(),
            briefing,
        }
    }
}
