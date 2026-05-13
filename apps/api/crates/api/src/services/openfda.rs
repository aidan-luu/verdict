//! openFDA `drug/drugsfda` ingestion client, record-to-row mapper, and
//! enrichment validator for Phase 3 PR A.
//!
//! The mapper does the *precise* selection of an application's original
//! approval (`submission_type = "ORIG"`, `submission_status = "AP"`) in
//! application code, because openFDA's Lucene-style search does **not**
//! correlate nested predicates within the same array element. A query
//! that filters by submission type, status, and date can therefore return
//! records whose ORIG submission is outside the requested window because
//! some unrelated SUPPL inside the same record matched the date. See
//! `docs/historical_events_curation.md`.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::time::Duration;

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use thiserror::Error;

const DEFAULT_BASE_URL: &str = "https://api.fda.gov";
const DRUGSFDA_PATH: &str = "/drug/drugsfda.json";
const LABEL_PATH: &str = "/drug/label.json";
const DEFAULT_PAGE_DELAY_MS: u64 = 250;
/// openFDA caps `limit` at 1000 and `skip + limit` at 26000. The 2010+
/// universe of original NDA/BLA approvals is well under that ceiling, so
/// we paginate without cursor logic.
pub const PAGE_LIMIT: u32 = 1000;
pub const MAX_SKIP: u32 = 25_000;

/// Indication-area vocabulary used by both the LLM-enrichment validator and
/// the DB CHECK constraint. Keep these in sync with the migration.
pub const INDICATION_AREAS: &[&str] = &[
    "oncology",
    "metabolic",
    "neurological",
    "cardiovascular",
    "infectious_disease",
    "immunology",
    "rare_disease",
    "other",
];

/// Primary-endpoint vocabulary. Enforced in Rust at write time (the DB
/// column is plain TEXT so we can evolve the vocabulary without a
/// migration).
pub const PRIMARY_ENDPOINT_TYPES: &[&str] = &[
    "overall_survival",
    "progression_free_survival",
    "response_rate",
    "biomarker",
    "functional",
    "patient_reported",
    "safety",
    "other",
];

pub const ADVISORY_COMMITTEE_VOTES: &[&str] = &["favorable", "mixed", "unfavorable"];

/// Application-type vocabulary. The DB CHECK constraints on both
/// `historical_event.application_type` and `events.application_type`
/// enforce this set; reuse the constant for Rust-side validation.
pub const APPLICATION_TYPES: &[&str] = &["NDA", "BLA", "ANDA", "other"];

/// Default LLM-confidence threshold per field during enrichment.
pub const DEFAULT_ENRICHMENT_CONFIDENCE_THRESHOLD: f32 = 0.7;

// ----------------------------------------------------------------------------
// Error types
// ----------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum OpenFdaError {
    #[error("openFDA configuration error: {0}")]
    Config(String),
    #[error("openFDA HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("openFDA returned status {status}: {message}")]
    Status { status: u16, message: String },
    #[error("openFDA response was not valid JSON: {0}")]
    Parse(#[from] serde_json::Error),
}

// ----------------------------------------------------------------------------
// Raw response types (preserve openFDA field names)
// ----------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct DrugsFdaPage {
    #[serde(default)]
    pub meta: Option<DrugsFdaMeta>,
    #[serde(default)]
    pub results: Vec<DrugsFdaRecord>,
}

#[derive(Debug, Deserialize)]
pub struct DrugsFdaMeta {
    #[serde(default)]
    pub results: Option<DrugsFdaMetaResults>,
}

#[derive(Debug, Deserialize)]
pub struct DrugsFdaMetaResults {
    pub skip: u32,
    pub limit: u32,
    pub total: u32,
}

/// A single openFDA `drug/drugsfda` record. `Serialize` is derived so the
/// binary can stash the raw response into `historical_event.raw_openfda_data`
/// for future re-processing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DrugsFdaRecord {
    pub application_number: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sponsor_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub openfda: Option<OpenFdaSection>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub products: Vec<DrugsFdaProduct>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub submissions: Vec<DrugsFdaSubmission>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenFdaSection {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brand_name: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generic_name: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub application_number: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DrugsFdaProduct {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brand_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DrugsFdaSubmission {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub submission_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub submission_number: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub submission_status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub submission_status_date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_priority: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct LabelPage {
    #[serde(default)]
    pub results: Vec<LabelRecord>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LabelRecord {
    #[serde(default)]
    pub effective_time: Option<String>,
    #[serde(default)]
    pub indications_and_usage: Option<Vec<String>>,
    #[serde(default)]
    pub clinical_studies: Option<Vec<String>>,
    #[serde(default)]
    pub description: Option<String>,
}

// ----------------------------------------------------------------------------
// Mapper outputs
// ----------------------------------------------------------------------------

/// What the mapper produces per input record. Either a row ready for
/// upsert into `historical_event`, or a skip with a structured reason so
/// the binary can log per-reason counters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MapOutcome {
    Insert(HistoricalEventInsert),
    Skipped(SkipReason),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    UnsupportedApplicationType { prefix: String },
    NoOriginalApproval,
    InvalidApprovalDate(String),
    DateOutOfWindow { approval_date: NaiveDate },
    MissingDrugName,
    MissingSponsor,
}

/// Fields populated from an openFDA record on first ingest. Enrichment
/// (`indication_area`, `primary_endpoint_type`, `advisory_committee_*`)
/// is filled in by a later pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoricalEventInsert {
    pub application_number: String,
    pub drug_name: String,
    pub sponsor_name: String,
    pub application_type: ApplicationType,
    pub approval_date: NaiveDate,
    pub review_priority: Option<ReviewPriority>,
    pub decision_outcome: DecisionOutcome,
    pub enrichment_status: EnrichmentStatus,
    pub source: HistoricalEventSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplicationType {
    Nda,
    Bla,
    Anda,
    Other,
}

impl ApplicationType {
    pub fn as_db_str(self) -> &'static str {
        match self {
            ApplicationType::Nda => "NDA",
            ApplicationType::Bla => "BLA",
            ApplicationType::Anda => "ANDA",
            ApplicationType::Other => "other",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewPriority {
    Priority,
    Standard,
}

impl ReviewPriority {
    pub fn as_db_str(self) -> &'static str {
        match self {
            ReviewPriority::Priority => "priority",
            ReviewPriority::Standard => "standard",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecisionOutcome {
    Approved,
    ApprovedWithRems,
    Crl,
}

impl DecisionOutcome {
    pub fn as_db_str(self) -> &'static str {
        match self {
            DecisionOutcome::Approved => "approved",
            DecisionOutcome::ApprovedWithRems => "approved_with_rems",
            DecisionOutcome::Crl => "crl",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnrichmentStatus {
    StructuredOnly,
    LlmEnriched,
    ManuallyReviewed,
}

impl EnrichmentStatus {
    pub fn as_db_str(self) -> &'static str {
        match self {
            EnrichmentStatus::StructuredOnly => "structured_only",
            EnrichmentStatus::LlmEnriched => "llm_enriched",
            EnrichmentStatus::ManuallyReviewed => "manually_reviewed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoricalEventSource {
    OpenFda,
    Manual,
}

impl HistoricalEventSource {
    pub fn as_db_str(self) -> &'static str {
        match self {
            HistoricalEventSource::OpenFda => "openfda",
            HistoricalEventSource::Manual => "manual",
        }
    }
}

// ----------------------------------------------------------------------------
// Derivations: prefix, drug name, original approval
// ----------------------------------------------------------------------------

/// Derive `application_type` from the `application_number` prefix.
/// `"NDA022264"` -> `Nda`, `"BLA761306"` -> `Bla`, `"ANDA202217"` -> `Anda`,
/// anything else -> `Other`.
pub fn derive_application_type(application_number: &str) -> ApplicationType {
    let upper = application_number.trim().to_ascii_uppercase();
    if upper.starts_with("NDA") {
        ApplicationType::Nda
    } else if upper.starts_with("BLA") {
        ApplicationType::Bla
    } else if upper.starts_with("ANDA") {
        ApplicationType::Anda
    } else {
        ApplicationType::Other
    }
}

/// Drug-name fallback chain: `openfda.brand_name[0]` then
/// `products[0].brand_name`. Returns `None` if neither has a non-empty
/// value, which the caller treats as a skip.
pub fn select_drug_name(record: &DrugsFdaRecord) -> Option<String> {
    if let Some(section) = record.openfda.as_ref() {
        if let Some(brand_names) = section.brand_name.as_ref() {
            if let Some(first) = brand_names.iter().find_map(|name| non_empty(name)) {
                return Some(first);
            }
        }
    }
    record
        .products
        .iter()
        .find_map(|product| product.brand_name.as_deref().and_then(non_empty))
}

/// Pick the original approval submission for this application: the
/// submission with `submission_type == "ORIG"` **and**
/// `submission_status == "AP"`. If there are multiple (rare; e.g.
/// re-approvals), pick the one with the earliest parseable
/// `submission_status_date`.
pub fn select_original_approval(submissions: &[DrugsFdaSubmission]) -> Option<&DrugsFdaSubmission> {
    submissions
        .iter()
        .filter(|submission| {
            matches_ignore_ascii_case(submission.submission_type.as_deref(), "ORIG")
                && matches_ignore_ascii_case(submission.submission_status.as_deref(), "AP")
        })
        .min_by_key(|submission| {
            submission
                .submission_status_date
                .as_deref()
                .and_then(parse_openfda_date)
                .map(|date| (0, date))
                .unwrap_or((1, NaiveDate::MIN))
        })
}

fn matches_ignore_ascii_case(value: Option<&str>, expected: &str) -> bool {
    value.is_some_and(|v| v.eq_ignore_ascii_case(expected))
}

fn non_empty(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Parse openFDA's `YYYYMMDD` date format. Returns `None` for any
/// non-parseable string, including the literal `null` JSON value that
/// already deserializes into `Option::None`.
pub fn parse_openfda_date(raw: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(raw.trim(), "%Y%m%d").ok()
}

fn parse_review_priority(raw: Option<&str>) -> Option<ReviewPriority> {
    let trimmed = raw?.trim();
    if trimmed.is_empty() {
        return None;
    }
    match trimmed.to_ascii_uppercase().as_str() {
        "PRIORITY" => Some(ReviewPriority::Priority),
        "STANDARD" => Some(ReviewPriority::Standard),
        _ => None,
    }
}

// ----------------------------------------------------------------------------
// Mapping
// ----------------------------------------------------------------------------

/// Inclusive `[from, to]` window applied to the **original approval
/// submission's date only**.
#[derive(Debug, Clone, Copy)]
pub struct ApprovalWindow {
    pub from: NaiveDate,
    pub to: NaiveDate,
}

/// Builds the `search` query parameter for `drug/drugsfda` pagination (NDA/BLA, ORIG+AP, date window).
///
/// openFDA uses Elasticsearch `query_string`; inclusive ranges must use `[low TO high]` with
/// spaces around `TO`. Using `+TO+` inside the brackets yields `parse_exception` (HTTP 500).
pub fn drugsfda_approval_search_query(window: ApprovalWindow) -> String {
    let from = window.from.format("%Y%m%d").to_string();
    let to = window.to.format("%Y%m%d").to_string();
    // openFDA uses Elasticsearch `query_string`. Prefer explicit `AND` / `OR` with spaces
    // to avoid ambiguity around `+` encoding/decoding in query parameters.
    format!(
        "(application_number:NDA* OR application_number:BLA*) AND \
submissions.submission_type:ORIG AND \
submissions.submission_status:AP AND \
submissions.submission_status_date:[{from} TO {to}]"
    )
}

/// Map one openFDA record to either an upsert row or a structured skip.
/// Note that this is a pure function: no I/O.
pub fn map_record(record: &DrugsFdaRecord, window: ApprovalWindow) -> MapOutcome {
    let application_type = derive_application_type(&record.application_number);
    if !matches!(
        application_type,
        ApplicationType::Nda | ApplicationType::Bla
    ) {
        return MapOutcome::Skipped(SkipReason::UnsupportedApplicationType {
            prefix: application_type.as_db_str().to_string(),
        });
    }

    let original = match select_original_approval(&record.submissions) {
        Some(submission) => submission,
        None => return MapOutcome::Skipped(SkipReason::NoOriginalApproval),
    };

    let raw_date = original.submission_status_date.as_deref().unwrap_or("");
    let approval_date = match parse_openfda_date(raw_date) {
        Some(date) => date,
        None => return MapOutcome::Skipped(SkipReason::InvalidApprovalDate(raw_date.to_string())),
    };

    if approval_date < window.from || approval_date > window.to {
        return MapOutcome::Skipped(SkipReason::DateOutOfWindow { approval_date });
    }

    let drug_name = match select_drug_name(record) {
        Some(name) => name,
        None => return MapOutcome::Skipped(SkipReason::MissingDrugName),
    };

    let sponsor_name = match record.sponsor_name.as_deref().and_then(|name| {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    }) {
        Some(name) => name,
        None => return MapOutcome::Skipped(SkipReason::MissingSponsor),
    };

    let review_priority = parse_review_priority(original.review_priority.as_deref());

    MapOutcome::Insert(HistoricalEventInsert {
        application_number: record.application_number.trim().to_string(),
        drug_name,
        sponsor_name,
        application_type,
        approval_date,
        review_priority,
        decision_outcome: DecisionOutcome::Approved,
        enrichment_status: EnrichmentStatus::StructuredOnly,
        source: HistoricalEventSource::OpenFda,
    })
}

// ----------------------------------------------------------------------------
// Enrichment validation
// ----------------------------------------------------------------------------

/// Validated enrichment fields, ready for an UPDATE. Any field that
/// failed validation (low confidence, out-of-vocabulary, missing) is
/// `None` and not written. The DB keeps its prior value (typically NULL).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EnrichmentUpdate {
    pub indication_area: Option<String>,
    pub primary_endpoint_type: Option<String>,
    pub advisory_committee_held: Option<bool>,
    pub advisory_committee_vote: Option<String>,
}

impl EnrichmentUpdate {
    pub fn any_field_present(&self) -> bool {
        self.indication_area.is_some()
            || self.primary_endpoint_type.is_some()
            || self.advisory_committee_held.is_some()
            || self.advisory_committee_vote.is_some()
    }
}

/// Raw LLM output shape for the enrichment prompt. Confidences are
/// per-field so we can accept what passes and discard what fails.
#[derive(Debug, Deserialize)]
pub struct EnrichmentLlmOutput {
    pub indication_area: Option<EnrichedString>,
    pub primary_endpoint_type: Option<EnrichedString>,
    pub advisory_committee_held: Option<EnrichedBool>,
    pub advisory_committee_vote: Option<EnrichedString>,
}

#[derive(Debug, Deserialize)]
pub struct EnrichedString {
    pub value: Option<String>,
    pub confidence: f32,
}

#[derive(Debug, Deserialize)]
pub struct EnrichedBool {
    pub value: Option<bool>,
    pub confidence: f32,
}

#[derive(Debug, Error)]
pub enum EnrichmentValidationError {
    #[error("enrichment payload could not be parsed as JSON: {0}")]
    InvalidJson(String),
}

/// Parse and validate an LLM enrichment response. The function is
/// permissive on a per-field basis: an unparseable field is dropped, not
/// errored, so partial enrichment is preserved.
pub fn parse_and_validate_enrichment(
    raw_json: &str,
    confidence_threshold: f32,
) -> Result<EnrichmentUpdate, EnrichmentValidationError> {
    let parsed: EnrichmentLlmOutput = serde_json::from_str(raw_json.trim())
        .map_err(|error| EnrichmentValidationError::InvalidJson(error.to_string()))?;

    let mut update = EnrichmentUpdate::default();

    if let Some(field) = parsed.indication_area {
        if field.confidence >= confidence_threshold {
            if let Some(value) = field
                .value
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(|v| v.to_ascii_lowercase())
            {
                if INDICATION_AREAS.contains(&value.as_str()) {
                    update.indication_area = Some(value);
                }
            }
        }
    }

    if let Some(field) = parsed.primary_endpoint_type {
        if field.confidence >= confidence_threshold {
            if let Some(value) = field
                .value
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(|v| v.to_ascii_lowercase())
            {
                if PRIMARY_ENDPOINT_TYPES.contains(&value.as_str()) {
                    update.primary_endpoint_type = Some(value);
                }
            }
        }
    }

    if let Some(field) = parsed.advisory_committee_held {
        if field.confidence >= confidence_threshold {
            if let Some(value) = field.value {
                update.advisory_committee_held = Some(value);
            }
        }
    }

    if let Some(field) = parsed.advisory_committee_vote {
        if field.confidence >= confidence_threshold {
            if let Some(value) = field
                .value
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(|v| v.to_ascii_lowercase())
            {
                if ADVISORY_COMMITTEE_VOTES.contains(&value.as_str()) {
                    update.advisory_committee_vote = Some(value);
                }
            }
        }
    }

    Ok(update)
}

// ----------------------------------------------------------------------------
// HTTP client
// ----------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct OpenFdaConfig {
    pub api_key: String,
    pub base_url: String,
    pub page_delay: Duration,
}

impl OpenFdaConfig {
    pub fn from_env() -> Result<Self, OpenFdaError> {
        let api_key = std::env::var("OPENFDA_API_KEY")
            .map_err(|_| OpenFdaError::Config(missing_api_key_message()))?;
        let api_key = api_key.trim().to_string();
        if api_key.is_empty() || api_key == "replace-with-real-key" {
            return Err(OpenFdaError::Config(missing_api_key_message()));
        }

        let base_url = std::env::var("OPENFDA_BASE_URL")
            .ok()
            .map(|value| value.trim().trim_end_matches('/').to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());

        let page_delay_ms = std::env::var("OPENFDA_PAGE_DELAY_MS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(DEFAULT_PAGE_DELAY_MS);

        Ok(Self {
            api_key,
            base_url,
            page_delay: Duration::from_millis(page_delay_ms),
        })
    }
}

fn missing_api_key_message() -> String {
    "OPENFDA_API_KEY is required. Sign up for a free key at \
     https://open.fda.gov/apis/authentication/ (without a key the daily \
     limit of 1,000 requests per IP cannot complete an ingestion run)."
        .to_string()
}

#[derive(Debug, Clone)]
pub struct OpenFdaClient {
    config: OpenFdaConfig,
    http: reqwest::Client,
}

impl OpenFdaClient {
    pub fn new(config: OpenFdaConfig, http: reqwest::Client) -> Self {
        Self { config, http }
    }

    pub fn page_delay(&self) -> Duration {
        self.config.page_delay
    }

    /// Server-side narrowing for the dataset: NDA or BLA only, with an
    /// `ORIG + AP + date-window` predicate. The latter is a *coarse*
    /// filter — openFDA does not correlate predicates within array
    /// elements, so the caller must re-check in Rust via `map_record`.
    pub async fn search_drugsfda(
        &self,
        window: ApprovalWindow,
        skip: u32,
        limit: u32,
    ) -> Result<DrugsFdaPage, OpenFdaError> {
        let limit = limit.min(PAGE_LIMIT);
        if skip > MAX_SKIP {
            return Err(OpenFdaError::Config(format!(
                "openFDA skip+limit cap exceeded (skip={skip}, max={MAX_SKIP})"
            )));
        }

        let search = drugsfda_approval_search_query(window);

        // #region agent log
        {
            let log_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../debug-2df113.log");
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_millis());
            let payload = serde_json::json!({
                "sessionId": "2df113",
                "runId": "post-fix",
                "hypothesisId": "H1-range-syntax",
                "location": "openfda.rs:search_drugsfda",
                "message": "constructed drugsfda search",
                "data": {
                    "search_len": search.len(),
                    "search": search.as_str(),
                    "bytes_40_55": search.as_bytes().get(40..55).map(|b| String::from_utf8_lossy(b).into_owned()),
                },
                "timestamp": ts,
            });
            if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(log_path) {
                let _ = writeln!(f, "{}", payload);
            }
        }
        // #endregion

        let url = format!("{}{}", self.config.base_url, DRUGSFDA_PATH);
        let response = self
            .http
            .get(&url)
            .query(&[
                ("api_key", self.config.api_key.as_str()),
                ("search", search.as_str()),
                ("limit", &limit.to_string()),
                ("skip", &skip.to_string()),
            ])
            .send()
            .await?;

        let status = response.status();
        let bytes = response.bytes().await?;
        if status == reqwest::StatusCode::NOT_FOUND {
            // openFDA returns 404 with `{ "error": { "code": "NOT_FOUND" } }`
            // when a search yields zero results. Treat that as an empty page.
            return Ok(DrugsFdaPage {
                meta: None,
                results: Vec::new(),
            });
        }
        if !status.is_success() {
            return Err(OpenFdaError::Status {
                status: status.as_u16(),
                message: String::from_utf8_lossy(&bytes).into_owned(),
            });
        }

        let page: DrugsFdaPage = serde_json::from_slice(&bytes)?;
        Ok(page)
    }

    /// Fetch the most recent prescribing-information label for an
    /// application number. Returns `None` when no label is found.
    pub async fn fetch_latest_label(
        &self,
        application_number: &str,
    ) -> Result<Option<LabelRecord>, OpenFdaError> {
        let trimmed = application_number.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }
        let search = format!("openfda.application_number:\"{trimmed}\"");
        let url = format!("{}{}", self.config.base_url, LABEL_PATH);

        let response = self
            .http
            .get(&url)
            .query(&[
                ("api_key", self.config.api_key.as_str()),
                ("search", search.as_str()),
                ("limit", "1"),
                ("sort", "effective_time:desc"),
            ])
            .send()
            .await?;

        let status = response.status();
        let bytes = response.bytes().await?;
        if status == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !status.is_success() {
            return Err(OpenFdaError::Status {
                status: status.as_u16(),
                message: String::from_utf8_lossy(&bytes).into_owned(),
            });
        }

        let page: LabelPage = serde_json::from_slice(&bytes)?;
        Ok(page.results.into_iter().next())
    }
}

// ----------------------------------------------------------------------------
// Tests
// ----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn window_2010_through_2026() -> ApprovalWindow {
        ApprovalWindow {
            from: NaiveDate::from_ymd_opt(2010, 1, 1).expect("date"),
            to: NaiveDate::from_ymd_opt(2026, 12, 31).expect("date"),
        }
    }

    #[test]
    fn drugsfda_search_range_uses_spaced_to_not_plus_wrapped_to() {
        let window = ApprovalWindow {
            from: NaiveDate::from_ymd_opt(2010, 1, 1).expect("date"),
            to: NaiveDate::from_ymd_opt(2026, 5, 13).expect("date"),
        };
        let query = drugsfda_approval_search_query(window);
        assert!(
            query.contains("[20100101 TO 20260513]"),
            "expected Lucene inclusive range with spaces around TO, got {query:?}"
        );
        assert!(
            !query.contains("+TO+"),
            "+TO+ inside a range breaks the query_string parser, got {query:?}"
        );
    }

    #[test]
    fn application_type_from_prefix() {
        assert_eq!(derive_application_type("NDA022264"), ApplicationType::Nda);
        assert_eq!(derive_application_type("nda022264"), ApplicationType::Nda);
        assert_eq!(derive_application_type("BLA761306"), ApplicationType::Bla);
        assert_eq!(derive_application_type("ANDA202217"), ApplicationType::Anda);
        assert_eq!(derive_application_type("XYZ123"), ApplicationType::Other);
        assert_eq!(derive_application_type(""), ApplicationType::Other);
    }

    #[test]
    fn drug_name_prefers_openfda_brand_name_then_products() {
        let with_openfda = DrugsFdaRecord {
            application_number: "NDA1".into(),
            sponsor_name: Some("Acme".into()),
            openfda: Some(OpenFdaSection {
                brand_name: Some(vec!["BRANDED".into(), "ALT".into()]),
                generic_name: None,
                application_number: None,
            }),
            products: vec![DrugsFdaProduct {
                brand_name: Some("FALLBACK".into()),
            }],
            submissions: vec![],
        };
        assert_eq!(select_drug_name(&with_openfda), Some("BRANDED".into()));

        let products_only = DrugsFdaRecord {
            application_number: "NDA1".into(),
            sponsor_name: Some("Acme".into()),
            openfda: None,
            products: vec![DrugsFdaProduct {
                brand_name: Some("PRODUCTBRAND".into()),
            }],
            submissions: vec![],
        };
        assert_eq!(
            select_drug_name(&products_only),
            Some("PRODUCTBRAND".into())
        );

        let neither = DrugsFdaRecord {
            application_number: "NDA1".into(),
            sponsor_name: Some("Acme".into()),
            openfda: Some(OpenFdaSection {
                brand_name: Some(vec!["   ".into()]),
                generic_name: None,
                application_number: None,
            }),
            products: vec![DrugsFdaProduct {
                brand_name: Some(" ".into()),
            }],
            submissions: vec![],
        };
        assert_eq!(select_drug_name(&neither), None);
    }

    fn submission(
        kind: &str,
        status: &str,
        date: &str,
        priority: Option<&str>,
    ) -> DrugsFdaSubmission {
        DrugsFdaSubmission {
            submission_type: Some(kind.into()),
            submission_number: Some("1".into()),
            submission_status: Some(status.into()),
            submission_status_date: Some(date.into()),
            review_priority: priority.map(Into::into),
        }
    }

    /// Regression: openFDA's search does not correlate nested predicates,
    /// so a record can come back with an old ORIG and a recent SUPPL
    /// matching the date filter. The selector must still pick the ORIG.
    #[test]
    fn original_approval_selector_ignores_recent_supplements() {
        let submissions = vec![
            submission("SUPPL", "AP", "20250430", Some("STANDARD")),
            submission("ORIG", "AP", "20120815", Some("STANDARD")),
            submission("SUPPL", "AP", "20180727", Some("STANDARD")),
        ];
        let selected = select_original_approval(&submissions).expect("ORIG should be selected");
        assert_eq!(selected.submission_type.as_deref(), Some("ORIG"));
        assert_eq!(selected.submission_status_date.as_deref(), Some("20120815"));
    }

    #[test]
    fn original_approval_selector_picks_earliest_when_multiple_origs() {
        let submissions = vec![
            submission("ORIG", "AP", "20140101", Some("STANDARD")),
            submission("ORIG", "AP", "20120815", Some("PRIORITY")),
        ];
        let selected = select_original_approval(&submissions).expect("ORIG should be selected");
        assert_eq!(selected.submission_status_date.as_deref(), Some("20120815"));
        assert_eq!(selected.review_priority.as_deref(), Some("PRIORITY"));
    }

    #[test]
    fn original_approval_selector_returns_none_without_orig_ap() {
        let submissions = vec![
            submission("SUPPL", "AP", "20250101", None),
            submission("ORIG", "TA", "20120101", None), // TA = tentative approval
        ];
        assert!(select_original_approval(&submissions).is_none());
    }

    fn record_with_submissions(submissions: Vec<DrugsFdaSubmission>) -> DrugsFdaRecord {
        DrugsFdaRecord {
            application_number: "NDA222222".into(),
            sponsor_name: Some("Acme Pharma".into()),
            openfda: Some(OpenFdaSection {
                brand_name: Some(vec!["ACMEDRUG".into()]),
                generic_name: None,
                application_number: Some(vec!["NDA222222".into()]),
            }),
            products: vec![DrugsFdaProduct {
                brand_name: Some("ACMEDRUG".into()),
            }],
            submissions,
        }
    }

    #[test]
    fn map_record_picks_orig_when_record_has_recent_supplement() {
        let record = record_with_submissions(vec![
            submission("SUPPL", "AP", "20250430", Some("STANDARD")),
            submission("ORIG", "AP", "20120815", Some("PRIORITY")),
        ]);

        let outcome = map_record(&record, window_2010_through_2026());
        let MapOutcome::Insert(row) = outcome else {
            panic!("expected Insert outcome, got {outcome:?}");
        };

        assert_eq!(row.application_number, "NDA222222");
        assert_eq!(row.drug_name, "ACMEDRUG");
        assert_eq!(row.sponsor_name, "Acme Pharma");
        assert_eq!(row.application_type, ApplicationType::Nda);
        assert_eq!(
            row.approval_date,
            NaiveDate::from_ymd_opt(2012, 8, 15).expect("date")
        );
        assert_eq!(row.review_priority, Some(ReviewPriority::Priority));
        assert_eq!(row.decision_outcome, DecisionOutcome::Approved);
        assert_eq!(row.enrichment_status, EnrichmentStatus::StructuredOnly);
        assert_eq!(row.source, HistoricalEventSource::OpenFda);
    }

    #[test]
    fn map_record_skips_when_orig_is_before_window() {
        let record = record_with_submissions(vec![
            submission("SUPPL", "AP", "20250123", Some("STANDARD")),
            submission("ORIG", "AP", "20090731", Some("STANDARD")),
        ]);

        let outcome = map_record(&record, window_2010_through_2026());
        match outcome {
            MapOutcome::Skipped(SkipReason::DateOutOfWindow { approval_date }) => {
                assert_eq!(
                    approval_date,
                    NaiveDate::from_ymd_opt(2009, 7, 31).expect("date")
                );
            }
            other => panic!("expected DateOutOfWindow skip, got {other:?}"),
        }
    }

    #[test]
    fn map_record_skips_anda_records() {
        let mut record = record_with_submissions(vec![submission("ORIG", "AP", "20150101", None)]);
        record.application_number = "ANDA999".into();

        let outcome = map_record(&record, window_2010_through_2026());
        assert!(matches!(
            outcome,
            MapOutcome::Skipped(SkipReason::UnsupportedApplicationType { .. })
        ));
    }

    #[test]
    fn map_record_skips_when_no_original_approval() {
        let record = record_with_submissions(vec![submission("SUPPL", "AP", "20200101", None)]);
        let outcome = map_record(&record, window_2010_through_2026());
        assert!(matches!(
            outcome,
            MapOutcome::Skipped(SkipReason::NoOriginalApproval)
        ));
    }

    #[test]
    fn map_record_skips_when_drug_name_missing() {
        let mut record = record_with_submissions(vec![submission("ORIG", "AP", "20150101", None)]);
        record.openfda = None;
        record.products = vec![];

        let outcome = map_record(&record, window_2010_through_2026());
        assert!(matches!(
            outcome,
            MapOutcome::Skipped(SkipReason::MissingDrugName)
        ));
    }

    #[test]
    fn map_record_skips_when_sponsor_missing() {
        let mut record = record_with_submissions(vec![submission("ORIG", "AP", "20150101", None)]);
        record.sponsor_name = Some("   ".into());

        let outcome = map_record(&record, window_2010_through_2026());
        assert!(matches!(
            outcome,
            MapOutcome::Skipped(SkipReason::MissingSponsor)
        ));
    }

    #[test]
    fn map_record_full_bla_fixture_round_trip() {
        let record: DrugsFdaRecord = serde_json::from_str(BLA_ORIG_FIXTURE).expect("fixture json");
        let outcome = map_record(&record, window_2010_through_2026());
        let MapOutcome::Insert(row) = outcome else {
            panic!("expected Insert outcome, got {outcome:?}");
        };
        assert_eq!(row.application_number, "BLA761306");
        assert_eq!(row.application_type, ApplicationType::Bla);
        assert_eq!(row.drug_name, "EBGLYSS");
        assert_eq!(
            row.approval_date,
            NaiveDate::from_ymd_opt(2024, 9, 13).expect("date")
        );
        assert_eq!(row.review_priority, Some(ReviewPriority::Standard));
    }

    const BLA_ORIG_FIXTURE: &str = r#"{
        "application_number": "BLA761306",
        "sponsor_name": "ELI LILLY AND CO",
        "openfda": {
            "application_number": ["BLA761306"],
            "brand_name": ["EBGLYSS"]
        },
        "products": [
            { "brand_name": "EBGLYSS" }
        ],
        "submissions": [
            {
                "submission_type": "SUPPL",
                "submission_number": "5",
                "submission_status": "AP",
                "submission_status_date": "20251028",
                "review_priority": "STANDARD"
            },
            {
                "submission_type": "ORIG",
                "submission_number": "1",
                "submission_status": "AP",
                "submission_status_date": "20240913",
                "review_priority": "STANDARD"
            }
        ]
    }"#;

    #[test]
    fn enrichment_validator_accepts_well_formed_payload() {
        let json = r#"{
            "indication_area":      { "value": "oncology",         "confidence": 0.92 },
            "primary_endpoint_type":{ "value": "overall_survival", "confidence": 0.81 },
            "advisory_committee_held":{ "value": true,             "confidence": 0.75 },
            "advisory_committee_vote":{ "value": "favorable",      "confidence": 0.71 }
        }"#;
        let update = parse_and_validate_enrichment(json, DEFAULT_ENRICHMENT_CONFIDENCE_THRESHOLD)
            .expect("valid payload");
        assert_eq!(update.indication_area.as_deref(), Some("oncology"));
        assert_eq!(
            update.primary_endpoint_type.as_deref(),
            Some("overall_survival")
        );
        assert_eq!(update.advisory_committee_held, Some(true));
        assert_eq!(update.advisory_committee_vote.as_deref(), Some("favorable"));
        assert!(update.any_field_present());
    }

    #[test]
    fn enrichment_validator_drops_low_confidence_fields() {
        let json = r#"{
            "indication_area": { "value": "oncology", "confidence": 0.69 },
            "primary_endpoint_type": { "value": "overall_survival", "confidence": 0.95 }
        }"#;
        let update = parse_and_validate_enrichment(json, DEFAULT_ENRICHMENT_CONFIDENCE_THRESHOLD)
            .expect("valid payload");
        assert!(update.indication_area.is_none());
        assert_eq!(
            update.primary_endpoint_type.as_deref(),
            Some("overall_survival")
        );
    }

    #[test]
    fn enrichment_validator_drops_out_of_vocabulary_values() {
        let json = r#"{
            "indication_area": { "value": "dentistry", "confidence": 0.99 },
            "advisory_committee_vote": { "value": "abstain", "confidence": 0.99 }
        }"#;
        let update = parse_and_validate_enrichment(json, DEFAULT_ENRICHMENT_CONFIDENCE_THRESHOLD)
            .expect("valid payload");
        assert!(update.indication_area.is_none());
        assert!(update.advisory_committee_vote.is_none());
        assert!(!update.any_field_present());
    }

    #[test]
    fn enrichment_validator_rejects_malformed_json() {
        let err =
            parse_and_validate_enrichment("not json", DEFAULT_ENRICHMENT_CONFIDENCE_THRESHOLD)
                .expect_err("should fail");
        assert!(matches!(err, EnrichmentValidationError::InvalidJson(_)));
    }

    #[test]
    fn enrichment_validator_handles_partial_payload() {
        let json = r#"{
            "indication_area": { "value": "cardiovascular", "confidence": 0.88 }
        }"#;
        let update = parse_and_validate_enrichment(json, DEFAULT_ENRICHMENT_CONFIDENCE_THRESHOLD)
            .expect("valid payload");
        assert_eq!(update.indication_area.as_deref(), Some("cardiovascular"));
        assert!(update.primary_endpoint_type.is_none());
        assert!(update.advisory_committee_held.is_none());
        assert!(update.advisory_committee_vote.is_none());
    }
}
