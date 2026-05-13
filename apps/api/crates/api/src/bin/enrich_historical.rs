//! Phase 3 PR A binary: pick `historical_event` rows at
//! `enrichment_status = 'structured_only'`, fetch each application's most
//! recent label from openFDA, and ask Gemini for structured enrichment
//! (indication area, endpoint type, advisory-committee posture) with
//! per-field confidence. Low-confidence or out-of-vocabulary fields are
//! discarded; surviving fields are written and the row's status is
//! promoted to `'llm_enriched'`.
//!
//! Usage:
//!   cargo run --bin enrich_historical -- --batch-size 50
//!   cargo run --bin enrich_historical -- --batch-size 25 --from-year 2018
//!   cargo run --bin enrich_historical -- --batch-size 25 --sponsor "Eli%"

use std::path::Path;
use std::time::Duration;

use reqwest::Client as HttpClient;
use serde_json::{json, Value as JsonValue};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use tracing::{info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use verdict_api::services::historical_event_repo::{
    apply_enrichment, fetch_structured_only_batch, HistoricalEventForEnrichment,
};
use verdict_api::services::openfda::{
    parse_and_validate_enrichment, EnrichmentUpdate, LabelRecord, OpenFdaClient, OpenFdaConfig,
    ADVISORY_COMMITTEE_VOTES, DEFAULT_ENRICHMENT_CONFIDENCE_THRESHOLD, INDICATION_AREAS,
    PRIMARY_ENDPOINT_TYPES,
};
use verdict_api::state::GeminiConfig;

#[derive(Debug)]
struct CliArgs {
    batch_size: i64,
    from_year: Option<i32>,
    sponsor: Option<String>,
}

fn parse_cli_args() -> Result<CliArgs, String> {
    let mut batch_size: i64 = 50;
    let mut from_year: Option<i32> = None;
    let mut sponsor: Option<String> = None;

    let raw: Vec<String> = std::env::args().skip(1).collect();
    let mut iter = raw.into_iter();
    while let Some(flag) = iter.next() {
        match flag.as_str() {
            "--batch-size" => {
                let value = iter
                    .next()
                    .ok_or_else(|| "--batch-size requires a value".to_string())?;
                let parsed = value
                    .parse::<i64>()
                    .map_err(|_| format!("--batch-size must be an integer, got {value:?}"))?;
                if parsed <= 0 || parsed > 500 {
                    return Err(format!(
                        "--batch-size must be between 1 and 500, got {parsed}"
                    ));
                }
                batch_size = parsed;
            }
            "--from-year" => {
                let value = iter
                    .next()
                    .ok_or_else(|| "--from-year requires a value".to_string())?;
                from_year = Some(
                    value
                        .parse::<i32>()
                        .map_err(|_| format!("--from-year must be an integer, got {value:?}"))?,
                );
            }
            "--sponsor" => {
                let value = iter
                    .next()
                    .ok_or_else(|| "--sponsor requires a value".to_string())?;
                sponsor = Some(value);
            }
            "--help" | "-h" => {
                eprintln!(
                    "enrich_historical [--batch-size N (default 50, max 500)] [--from-year YYYY] [--sponsor \"ILIKE pattern\"]"
                );
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(CliArgs {
        batch_size,
        from_year,
        sponsor,
    })
}

#[derive(Default, Debug)]
struct EnrichCounters {
    seen: u64,
    enriched: u64,
    skipped_no_label: u64,
    skipped_validation: u64,
    errors: u64,
    fields_indication: u64,
    fields_endpoint: u64,
    fields_adcom_held: u64,
    fields_adcom_vote: u64,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::from_path(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.env")).ok();

    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let cli = match parse_cli_args() {
        Ok(args) => args,
        Err(message) => {
            eprintln!("error: {message}");
            std::process::exit(2);
        }
    };

    let database_url = std::env::var("DATABASE_URL")?;
    let openfda_config = OpenFdaConfig::from_env()?;
    let gemini_api_key = std::env::var("GEMINI_API_KEY")
        .map_err(|_| "GEMINI_API_KEY is required for the enrichment binary".to_string())?;
    let gemini_model =
        std::env::var("GEMINI_MODEL").unwrap_or_else(|_| GeminiConfig::DEFAULT_MODEL.to_string());

    let http = HttpClient::builder()
        .timeout(Duration::from_secs(60))
        .build()?;
    let openfda_client = OpenFdaClient::new(openfda_config, http.clone());

    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&database_url)
        .await?;

    let batch =
        fetch_structured_only_batch(&pool, cli.batch_size, cli.from_year, cli.sponsor.as_deref())
            .await?;
    info!(batch_size = batch.len(), "loaded structured-only batch");

    let gemini = GeminiHelper {
        http: http.clone(),
        api_key: gemini_api_key,
        model: gemini_model,
    };

    let mut counters = EnrichCounters::default();
    for record in batch {
        counters.seen += 1;
        match enrich_one(&pool, &openfda_client, &gemini, &record).await {
            Ok(EnrichmentSuccess::Applied(update)) => {
                counters.enriched += 1;
                if update.indication_area.is_some() {
                    counters.fields_indication += 1;
                }
                if update.primary_endpoint_type.is_some() {
                    counters.fields_endpoint += 1;
                }
                if update.advisory_committee_held.is_some() {
                    counters.fields_adcom_held += 1;
                }
                if update.advisory_committee_vote.is_some() {
                    counters.fields_adcom_vote += 1;
                }
            }
            Ok(EnrichmentSuccess::NoFieldsWritten) => counters.skipped_validation += 1,
            Err(EnrichOneError::NoLabel) => {
                counters.skipped_no_label += 1;
                info!(
                    application_number = %record.application_number,
                    "no label available; leaving structured_only"
                );
            }
            Err(EnrichOneError::Other(message)) => {
                counters.errors += 1;
                warn!(application_number = %record.application_number, %message, "enrichment failed");
            }
        }
        tokio::time::sleep(openfda_client.page_delay()).await;
    }

    info!(?counters, "enrichment complete");
    Ok(())
}

enum EnrichmentSuccess {
    Applied(EnrichmentUpdate),
    NoFieldsWritten,
}

#[derive(Debug)]
enum EnrichOneError {
    NoLabel,
    Other(String),
}

async fn enrich_one(
    pool: &PgPool,
    openfda_client: &OpenFdaClient,
    gemini: &GeminiHelper,
    record: &HistoricalEventForEnrichment,
) -> Result<EnrichmentSuccess, EnrichOneError> {
    let label = openfda_client
        .fetch_latest_label(&record.application_number)
        .await
        .map_err(|error| EnrichOneError::Other(format!("label fetch failed: {error}")))?
        .ok_or(EnrichOneError::NoLabel)?;

    let text = label_text_blob(&label);
    if text.trim().is_empty() {
        return Err(EnrichOneError::NoLabel);
    }

    let raw = gemini
        .request_enrichment(record, &text)
        .await
        .map_err(EnrichOneError::Other)?;

    let update = parse_and_validate_enrichment(&raw, DEFAULT_ENRICHMENT_CONFIDENCE_THRESHOLD)
        .map_err(|error| EnrichOneError::Other(error.to_string()))?;

    if !update.any_field_present() {
        return Ok(EnrichmentSuccess::NoFieldsWritten);
    }

    apply_enrichment(pool, record.id, &update)
        .await
        .map_err(|error| EnrichOneError::Other(error.to_string()))?;
    Ok(EnrichmentSuccess::Applied(update))
}

fn label_text_blob(label: &LabelRecord) -> String {
    let mut buffer = String::new();
    if let Some(parts) = &label.indications_and_usage {
        for part in parts {
            buffer.push_str(part);
            buffer.push_str("\n\n");
        }
    }
    if let Some(parts) = &label.clinical_studies {
        for part in parts {
            buffer.push_str(part);
            buffer.push_str("\n\n");
        }
    }
    if let Some(description) = &label.description {
        buffer.push_str(description);
    }
    buffer
}

struct GeminiHelper {
    http: HttpClient,
    api_key: String,
    model: String,
}

impl GeminiHelper {
    async fn request_enrichment(
        &self,
        record: &HistoricalEventForEnrichment,
        label_text: &str,
    ) -> Result<String, String> {
        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent",
            model = self.model
        );
        let prompt = enrichment_prompt(record, label_text);
        let body = json!({
            "systemInstruction": { "parts": [{ "text": system_instruction() }] },
            "contents": [{ "role": "user", "parts": [{ "text": prompt }] }],
            "generationConfig": {
                "temperature": 0.0,
                "topP": 0.95,
                "topK": 1,
                "responseMimeType": "application/json",
                "responseSchema": enrichment_schema()
            }
        });

        let response = self
            .http
            .post(url)
            .query(&[("key", self.api_key.as_str())])
            .json(&body)
            .send()
            .await
            .map_err(|error| format!("gemini request failed: {error}"))?;

        let status = response.status();
        let bytes = response
            .bytes()
            .await
            .map_err(|error| format!("gemini response read failed: {error}"))?;
        if !status.is_success() {
            return Err(format!(
                "gemini status {status}: {body}",
                body = String::from_utf8_lossy(&bytes)
            ));
        }

        let parsed: JsonValue = serde_json::from_slice(&bytes)
            .map_err(|error| format!("gemini response was not JSON: {error}"))?;
        let text = parsed
            .pointer("/candidates/0/content/parts/0/text")
            .and_then(|value| value.as_str())
            .ok_or_else(|| "gemini returned no text candidate".to_string())?;

        Ok(text.to_string())
    }
}

fn system_instruction() -> &'static str {
    "You enrich FDA drug-approval metadata from prescribing-information labels. \
Return only JSON matching the schema. Be conservative: when a field is not clearly \
supported by the label text, return null for the value and a low confidence. Never \
invent advisory-committee outcomes — prescribing labels almost never include them, \
so expect advisory_committee_held and advisory_committee_vote to be null in most \
cases."
}

fn enrichment_prompt(record: &HistoricalEventForEnrichment, label_text: &str) -> String {
    format!(
        "Application: {application_number}\nDrug: {drug_name}\nSponsor: {sponsor}\n\n\
Label text (truncated by the caller):\n---\n{label}\n---\n\n\
Pick the indication_area from this vocabulary: {indication_areas}.\n\
Pick the primary_endpoint_type from: {endpoint_types}.\n\
For advisory_committee_held: true if the label explicitly references an FDA \
advisory-committee meeting, false if it explicitly states none was held, null \
otherwise.\n\
For advisory_committee_vote (only if held): one of {votes}.",
        application_number = record.application_number,
        drug_name = record.drug_name,
        sponsor = record.sponsor_name,
        label = truncate_for_prompt(label_text, 30_000),
        indication_areas = INDICATION_AREAS.join(", "),
        endpoint_types = PRIMARY_ENDPOINT_TYPES.join(", "),
        votes = ADVISORY_COMMITTEE_VOTES.join(", "),
    )
}

fn truncate_for_prompt(text: &str, max_chars: usize) -> &str {
    if text.chars().count() <= max_chars {
        return text;
    }
    let mut byte_end = 0;
    for (index, (byte_index, _)) in text.char_indices().enumerate() {
        if index >= max_chars {
            break;
        }
        byte_end = byte_index;
    }
    &text[..byte_end]
}

fn enrichment_schema() -> JsonValue {
    let string_with_confidence = json!({
        "type": "OBJECT",
        "properties": {
            "value": { "type": "STRING", "nullable": true },
            "confidence": { "type": "NUMBER" }
        },
        "required": ["confidence"]
    });
    let bool_with_confidence = json!({
        "type": "OBJECT",
        "properties": {
            "value": { "type": "BOOLEAN", "nullable": true },
            "confidence": { "type": "NUMBER" }
        },
        "required": ["confidence"]
    });
    json!({
        "type": "OBJECT",
        "properties": {
            "indication_area": string_with_confidence,
            "primary_endpoint_type": string_with_confidence,
            "advisory_committee_held": bool_with_confidence,
            "advisory_committee_vote": string_with_confidence
        },
        "required": [
            "indication_area",
            "primary_endpoint_type",
            "advisory_committee_held",
            "advisory_committee_vote"
        ]
    })
}
