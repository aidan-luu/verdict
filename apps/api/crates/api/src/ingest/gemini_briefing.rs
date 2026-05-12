//! Gemini-backed extraction of structured FDA PDUFA fields from a briefing PDF.
//!
//! Uses the Generative Language API with `responseMimeType: application/json` and a response
//! schema so the model returns machine-parseable output we validate in Rust before any DB write.

use async_trait::async_trait;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use chrono::NaiveDate;
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use tracing::warn;

use crate::error::AppError;
use crate::state::GeminiConfig;

/// User-visible title: drug plus PDUFA date (stable list ordering per Phase 2 plan).
pub fn derive_briefing_event_title(drug_name: &str, decision_date: NaiveDate) -> String {
    format!("{drug_name} PDUFA {decision_date}")
}

/// Validated fields ready for `INSERT INTO events` (Phase 2 briefing path).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BriefingExtractionPayload {
    pub drug_name: String,
    pub sponsor: String,
    pub indication: String,
    pub decision_date: NaiveDate,
    pub advisory_committee_date: Option<NaiveDate>,
    pub primary_endpoint: Option<String>,
    pub advisory_committee_vote: Option<String>,
}

/// Raw JSON from Gemini before date coercion and bounded string checks.
#[derive(Debug, Deserialize)]
struct BriefingLlmRecord {
    drug_name: String,
    sponsor: String,
    indication: String,
    /// ISO `YYYY-MM-DD` PDUFA / regulatory action date stated or implied in the document.
    decision_date: String,
    advisory_committee_date: Option<String>,
    primary_endpoint: Option<String>,
    advisory_committee_vote: Option<String>,
}

fn validate_bounded_text(label: &str, value: &str, min: usize, max: usize) -> Result<(), AppError> {
    let len = value.chars().count();
    if len < min || len > max {
        return Err(AppError::BadRequest(format!(
            "{label} length must be between {min} and {max} characters"
        )));
    }
    Ok(())
}

fn empty_optional_string(value: Option<String>) -> Option<String> {
    value.and_then(|raw| {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn parse_iso_date(label: &str, raw: &str) -> Result<NaiveDate, AppError> {
    NaiveDate::parse_from_str(raw.trim(), "%Y-%m-%d").map_err(|_| {
        AppError::BadRequest(format!(
            "{label} must be an ISO date YYYY-MM-DD, got {raw:?}"
        ))
    })
}

fn parse_optional_iso_date(
    label: &str,
    raw: &Option<String>,
) -> Result<Option<NaiveDate>, AppError> {
    match raw {
        None => Ok(None),
        Some(value) if value.trim().is_empty() => Ok(None),
        Some(value) => Ok(Some(parse_iso_date(label, value)?)),
    }
}

/// Parses Gemini JSON text, validates bounded strings and dates, maps into DB-ready payload.
pub fn parse_and_validate_briefing_json(text: &str) -> Result<BriefingExtractionPayload, AppError> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(AppError::BadRequest(
            "model returned empty JSON text".to_string(),
        ));
    }

    let mut record: BriefingLlmRecord = serde_json::from_str(trimmed).map_err(|error| {
        AppError::BadRequest(format!("model JSON did not match briefing schema: {error}"))
    })?;

    record.drug_name = record.drug_name.trim().to_string();
    record.sponsor = record.sponsor.trim().to_string();
    record.indication = record.indication.trim().to_string();
    record.decision_date = record.decision_date.trim().to_string();
    record.advisory_committee_date = empty_optional_string(record.advisory_committee_date);
    record.primary_endpoint = empty_optional_string(record.primary_endpoint);
    record.advisory_committee_vote = empty_optional_string(record.advisory_committee_vote);

    validate_bounded_text("drug_name", &record.drug_name, 1, 500)?;
    validate_bounded_text("sponsor", &record.sponsor, 1, 500)?;
    validate_bounded_text("indication", &record.indication, 1, 2000)?;
    if let Some(ref endpoint) = record.primary_endpoint {
        validate_bounded_text("primary_endpoint", endpoint, 1, 4000)?;
    }
    if let Some(ref vote) = record.advisory_committee_vote {
        validate_bounded_text("advisory_committee_vote", vote, 1, 2000)?;
    }

    let decision_date = parse_iso_date("decision_date", &record.decision_date)?;
    let advisory_committee_date =
        parse_optional_iso_date("advisory_committee_date", &record.advisory_committee_date)?;

    Ok(BriefingExtractionPayload {
        drug_name: record.drug_name,
        sponsor: record.sponsor,
        indication: record.indication,
        decision_date,
        advisory_committee_date,
        primary_endpoint: record.primary_endpoint,
        advisory_committee_vote: record.advisory_committee_vote,
    })
}

fn briefing_response_schema() -> serde_json::Value {
    json!({
        "type": "OBJECT",
        "properties": {
            "drug_name": { "type": "STRING", "description": "Non-proprietary or established drug name as used in the briefing." },
            "sponsor": { "type": "STRING", "description": "Applicant / sponsor company name." },
            "indication": { "type": "STRING", "description": "Primary proposed indication in concise clinical language." },
            "decision_date": { "type": "STRING", "description": "PDUFA goal date or stated regulatory action date as YYYY-MM-DD." },
            "advisory_committee_date": { "type": "STRING", "nullable": true, "description": "Advisory committee meeting date YYYY-MM-DD if present, else null." },
            "primary_endpoint": { "type": "STRING", "nullable": true, "description": "Primary efficacy endpoint if clearly stated, else null." },
            "advisory_committee_vote": { "type": "STRING", "nullable": true, "description": "Advisory committee vote summary if stated, else null." }
        },
        "required": ["drug_name", "sponsor", "indication", "decision_date"]
    })
}

fn briefing_system_instruction() -> &'static str {
    "You extract structured regulatory metadata from FDA drug briefing PDFs. \
Return only JSON matching the schema. Use ISO dates YYYY-MM-DD. \
If a field is unknown or not stated in the document, use null for optional fields. \
Never invent a PDUFA date: it must appear or be clearly implied in the document text or tables. \
Prefer the primary drug under review when multiple drugs appear."
}

#[async_trait]
pub trait BriefingExtractor: Send + Sync {
    async fn extract_structured_briefing(
        &self,
        pdf_bytes: &[u8],
        source_url: &str,
    ) -> Result<BriefingExtractionPayload, AppError>;
}

/// Production path: Gemini `generateContent` with PDF inline data and JSON response schema.
#[derive(Clone)]
pub struct LiveGeminiBriefing {
    gemini: GeminiConfig,
    client: Client,
}

impl LiveGeminiBriefing {
    pub fn new(gemini: GeminiConfig, client: Client) -> Self {
        Self { gemini, client }
    }

    async fn call_gemini_once(
        &self,
        pdf_base64: &str,
        source_url: &str,
    ) -> Result<String, AppError> {
        let model = self.gemini.model.trim();
        if model.is_empty() {
            return Err(AppError::BadRequest("GEMINI_MODEL is empty".to_string()));
        }

        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent"
        );

        let body = json!({
            "systemInstruction": {
                "parts": [{ "text": briefing_system_instruction() }]
            },
            "contents": [{
                "role": "user",
                "parts": [
                    {
                        "text": format!(
                            "Source PDF URL (context only; extract from the attached PDF bytes): {source_url}\n\nExtract the briefing fields as JSON."
                        )
                    },
                    {
                        "inlineData": {
                            "mimeType": "application/pdf",
                            "data": pdf_base64
                        }
                    }
                ]
            }],
            "generationConfig": {
                "temperature": 0.0,
                "topP": 0.95,
                "topK": 1,
                "responseMimeType": "application/json",
                "responseSchema": briefing_response_schema()
            }
        });

        let response = self
            .client
            .post(url)
            .query(&[("key", self.gemini.api_key.as_str())])
            .json(&body)
            .send()
            .await
            .map_err(|_| AppError::BadRequest("could not reach Gemini API".to_string()))?;

        let status = response.status();
        let bytes = response.bytes().await.map_err(|_| AppError::Internal)?;
        if !status.is_success() {
            let message = gemini_error_message(&bytes, status.as_u16());
            return Err(AppError::BadRequest(message));
        }

        let parsed: GeminiGenerateResponse = serde_json::from_slice(&bytes)
            .map_err(|_| AppError::BadRequest("Gemini response was not valid JSON".to_string()))?;

        let text = parsed
            .candidate_text()
            .ok_or_else(|| AppError::BadRequest("Gemini returned no text candidate".to_string()))?;

        Ok(text)
    }
}

#[derive(Deserialize)]
struct GeminiErrorBody {
    error: Option<GeminiErrorMessage>,
}

#[derive(Deserialize)]
struct GeminiErrorMessage {
    message: Option<String>,
}

fn gemini_error_message(body: &[u8], status: u16) -> String {
    let parsed: Option<GeminiErrorBody> = serde_json::from_slice(body).ok();
    let msg = parsed
        .as_ref()
        .and_then(|wrapper| wrapper.error.as_ref())
        .and_then(|error| error.message.clone())
        .unwrap_or_else(|| String::from_utf8_lossy(body).into_owned());

    format!("Gemini API error ({status}): {msg}")
}

#[derive(Debug, Deserialize)]
struct GeminiGenerateResponse {
    candidates: Option<Vec<GeminiCandidate>>,
}

#[derive(Debug, Deserialize)]
struct GeminiCandidate {
    content: Option<GeminiContent>,
}

#[derive(Debug, Deserialize)]
struct GeminiContent {
    parts: Option<Vec<GeminiPart>>,
}

#[derive(Debug, Deserialize)]
struct GeminiPart {
    text: Option<String>,
}

impl GeminiGenerateResponse {
    fn candidate_text(&self) -> Option<String> {
        let candidates = self.candidates.as_ref()?;
        let first = candidates.first()?;
        let parts = first.content.as_ref()?.parts.as_ref()?;
        let mut out = String::new();
        for part in parts {
            if let Some(t) = &part.text {
                out.push_str(t);
            }
        }
        let trimmed = out.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    }
}

#[async_trait]
impl BriefingExtractor for LiveGeminiBriefing {
    async fn extract_structured_briefing(
        &self,
        pdf_bytes: &[u8],
        source_url: &str,
    ) -> Result<BriefingExtractionPayload, AppError> {
        let pdf_base64 = STANDARD.encode(pdf_bytes);
        let mut last_error: Option<AppError> = None;

        for attempt in 0u8..3 {
            match self.call_gemini_once(&pdf_base64, source_url).await {
                Ok(text) => match parse_and_validate_briefing_json(&text) {
                    Ok(payload) => return Ok(payload),
                    Err(error) => {
                        warn!(
                            attempt,
                            error = %error,
                            "briefing JSON validation failed; retrying Gemini call"
                        );
                        last_error = Some(error);
                    }
                },
                Err(error) => {
                    // Transport / HTTP-level failures are not repaired by retrying the same payload.
                    return Err(error);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            AppError::BadRequest("briefing extraction failed after retries".to_string())
        }))
    }
}

/// Test / CI double: returns deterministic extraction without calling Gemini.
#[derive(Clone, Default)]
pub struct StubBriefingExtractor;

#[async_trait]
impl BriefingExtractor for StubBriefingExtractor {
    async fn extract_structured_briefing(
        &self,
        _pdf_bytes: &[u8],
        _source_url: &str,
    ) -> Result<BriefingExtractionPayload, AppError> {
        let decision_date = NaiveDate::parse_from_str("2026-12-01", "%Y-%m-%d")
            .map_err(|_| AppError::BadRequest("stub briefing date parse".to_string()))?;
        Ok(BriefingExtractionPayload {
            drug_name: "StubDrug".to_string(),
            sponsor: "StubSponsor".to_string(),
            indication: "Stub indication for automated tests.".to_string(),
            decision_date,
            advisory_committee_date: None,
            primary_endpoint: None,
            advisory_committee_vote: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_committed_fixture_for_regression() {
        let json = include_str!("../../tests/fixtures/briefing_valid_min.json");
        let parsed = parse_and_validate_briefing_json(json).expect("fixture should parse");
        assert_eq!(parsed.drug_name, "FixtureDrug");
    }

    #[test]
    fn parse_accepts_minimal_valid_payload() {
        let json = r#"{
            "drug_name": "Examplefilgrastim",
            "sponsor": "Example Pharma",
            "indication": "Neutropenia prophylaxis",
            "decision_date": "2026-08-15"
        }"#;
        let parsed = parse_and_validate_briefing_json(json).expect("valid");
        assert_eq!(parsed.drug_name, "Examplefilgrastim");
        assert_eq!(
            parsed.decision_date,
            NaiveDate::parse_from_str("2026-08-15", "%Y-%m-%d").expect("date")
        );
        assert!(parsed.advisory_committee_date.is_none());
    }

    #[test]
    fn parse_rejects_invalid_date() {
        let json = r#"{
            "drug_name": "X",
            "sponsor": "Y",
            "indication": "Z",
            "decision_date": "not-a-date"
        }"#;
        assert!(parse_and_validate_briefing_json(json).is_err());
    }

    #[test]
    fn derive_title_formats_pdufa() {
        let title = derive_briefing_event_title(
            "DrugA",
            NaiveDate::parse_from_str("2027-01-02", "%Y-%m-%d").expect("date"),
        );
        assert_eq!(title, "DrugA PDUFA 2027-01-02");
    }
}
