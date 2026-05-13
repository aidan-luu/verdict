//! Admin override endpoint for `historical_event` rows.
//!
//! `POST /admin/historical_events/{id}` lets the operator manually correct
//! or augment any structured field on a historical row, flipping
//! `enrichment_status` to `'manually_reviewed'`. This is the only path by
//! which a `decision_outcome = 'crl'` enters the dataset, because openFDA
//! `drug/drugsfda` does not surface Complete Response Letters.
//!
//! TODO(P1/P3): when Clerk JWT middleware lands on the rest of the API,
//! this route inherits the same gating. Until then, the single-user
//! deployment is the only operator.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

use crate::error::AppError;
use crate::services::historical_event_repo::{apply_manual_override, ManualOverride};
use crate::services::openfda::{
    DecisionOutcome, ADVISORY_COMMITTEE_VOTES, INDICATION_AREAS, PRIMARY_ENDPOINT_TYPES,
};
use crate::state::AppState;

#[derive(Debug, Deserialize, Validate)]
pub struct ManualOverrideRequest {
    #[validate(custom(function = "validate_non_empty_opt"))]
    pub drug_name: Option<String>,
    #[validate(custom(function = "validate_non_empty_opt"))]
    pub sponsor_name: Option<String>,
    #[validate(custom(function = "validate_indication_area"))]
    pub indication_area: Option<String>,
    #[validate(custom(function = "validate_primary_endpoint_type"))]
    pub primary_endpoint_type: Option<String>,
    pub advisory_committee_held: Option<bool>,
    #[validate(custom(function = "validate_advisory_committee_vote"))]
    pub advisory_committee_vote: Option<String>,
    #[validate(custom(function = "validate_decision_outcome"))]
    pub decision_outcome: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct HistoricalEventResponse {
    pub id: Uuid,
    pub application_number: String,
    pub drug_name: String,
    pub sponsor_name: String,
    pub application_type: String,
    pub approval_date: NaiveDate,
    pub review_priority: Option<String>,
    pub indication_area: Option<String>,
    pub primary_endpoint_type: Option<String>,
    pub advisory_committee_held: Option<bool>,
    pub advisory_committee_vote: Option<String>,
    pub decision_outcome: String,
    pub enrichment_status: String,
    pub source: String,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub async fn override_historical_event_handler(
    State(state): State<AppState>,
    Path(historical_event_id): Path<Uuid>,
    Json(payload): Json<ManualOverrideRequest>,
) -> Result<(StatusCode, Json<HistoricalEventResponse>), AppError> {
    payload.validate()?;
    if !has_any_field(&payload) {
        return Err(AppError::BadRequest(
            "override payload must include at least one field".to_string(),
        ));
    }

    let decision_outcome = payload
        .decision_outcome
        .as_deref()
        .map(parse_decision_outcome)
        .transpose()?;
    let override_values = ManualOverride {
        drug_name: payload.drug_name.map(trim_to_string),
        sponsor_name: payload.sponsor_name.map(trim_to_string),
        indication_area: payload.indication_area.map(normalize_lower),
        primary_endpoint_type: payload.primary_endpoint_type.map(normalize_lower),
        advisory_committee_held: payload.advisory_committee_held,
        advisory_committee_vote: payload.advisory_committee_vote.map(normalize_lower),
        decision_outcome,
        notes: payload.notes.map(trim_to_string),
    };

    let updated = apply_manual_override(&state.pool, historical_event_id, &override_values).await?;
    if !updated {
        return Err(AppError::NotFound("historical_event not found".to_string()));
    }

    let response = fetch_response(&state, historical_event_id).await?;
    Ok((StatusCode::OK, Json(response)))
}

async fn fetch_response(state: &AppState, id: Uuid) -> Result<HistoricalEventResponse, AppError> {
    let row = sqlx::query!(
        r#"
        SELECT
            id,
            application_number,
            drug_name,
            sponsor_name,
            application_type,
            approval_date,
            review_priority,
            indication_area,
            primary_endpoint_type,
            advisory_committee_held,
            advisory_committee_vote,
            decision_outcome,
            enrichment_status,
            source,
            notes,
            created_at,
            updated_at
        FROM historical_event
        WHERE id = $1
        "#,
        id
    )
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound("historical_event not found".to_string()))?;

    Ok(HistoricalEventResponse {
        id: row.id,
        application_number: row.application_number,
        drug_name: row.drug_name,
        sponsor_name: row.sponsor_name,
        application_type: row.application_type,
        approval_date: row.approval_date,
        review_priority: row.review_priority,
        indication_area: row.indication_area,
        primary_endpoint_type: row.primary_endpoint_type,
        advisory_committee_held: row.advisory_committee_held,
        advisory_committee_vote: row.advisory_committee_vote,
        decision_outcome: row.decision_outcome,
        enrichment_status: row.enrichment_status,
        source: row.source,
        notes: row.notes,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

fn has_any_field(payload: &ManualOverrideRequest) -> bool {
    payload.drug_name.is_some()
        || payload.sponsor_name.is_some()
        || payload.indication_area.is_some()
        || payload.primary_endpoint_type.is_some()
        || payload.advisory_committee_held.is_some()
        || payload.advisory_committee_vote.is_some()
        || payload.decision_outcome.is_some()
        || payload.notes.is_some()
}

fn trim_to_string(value: String) -> String {
    value.trim().to_string()
}

fn normalize_lower(value: String) -> String {
    value.trim().to_ascii_lowercase()
}

fn parse_decision_outcome(value: &str) -> Result<DecisionOutcome, AppError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "approved" => Ok(DecisionOutcome::Approved),
        "approved_with_rems" => Ok(DecisionOutcome::ApprovedWithRems),
        "crl" => Ok(DecisionOutcome::Crl),
        other => Err(AppError::BadRequest(format!(
            "invalid decision_outcome: {other}"
        ))),
    }
}

fn validate_non_empty_opt(value: &str) -> Result<(), validator::ValidationError> {
    if value.trim().is_empty() {
        return Err(validator::ValidationError::new("non_empty"));
    }
    Ok(())
}

fn validate_indication_area(value: &str) -> Result<(), validator::ValidationError> {
    let normalized = value.trim().to_ascii_lowercase();
    if INDICATION_AREAS.contains(&normalized.as_str()) {
        return Ok(());
    }
    Err(validator::ValidationError::new("indication_area"))
}

fn validate_primary_endpoint_type(value: &str) -> Result<(), validator::ValidationError> {
    let normalized = value.trim().to_ascii_lowercase();
    if PRIMARY_ENDPOINT_TYPES.contains(&normalized.as_str()) {
        return Ok(());
    }
    Err(validator::ValidationError::new("primary_endpoint_type"))
}

fn validate_advisory_committee_vote(value: &str) -> Result<(), validator::ValidationError> {
    let normalized = value.trim().to_ascii_lowercase();
    if ADVISORY_COMMITTEE_VOTES.contains(&normalized.as_str()) {
        return Ok(());
    }
    Err(validator::ValidationError::new("advisory_committee_vote"))
}

fn validate_decision_outcome(value: &str) -> Result<(), validator::ValidationError> {
    let normalized = value.trim().to_ascii_lowercase();
    if matches!(
        normalized.as_str(),
        "approved" | "approved_with_rems" | "crl"
    ) {
        return Ok(());
    }
    Err(validator::ValidationError::new("decision_outcome"))
}

#[cfg(test)]
mod tests {
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use chrono::NaiveDate;
    use sqlx::PgPool;
    use tower::util::ServiceExt;

    use crate::services::historical_event_repo::upsert_from_openfda;
    use crate::services::openfda::{
        ApplicationType, DecisionOutcome, EnrichmentStatus, HistoricalEventInsert,
        HistoricalEventSource, ReviewPriority,
    };
    use crate::{app::router, state::AppState};

    fn sample_insert(application_number: &str) -> HistoricalEventInsert {
        HistoricalEventInsert {
            application_number: application_number.to_string(),
            drug_name: "ACMEDRUG".to_string(),
            sponsor_name: "Acme Pharma".to_string(),
            application_type: ApplicationType::Nda,
            approval_date: NaiveDate::from_ymd_opt(2020, 6, 1).expect("date"),
            review_priority: Some(ReviewPriority::Standard),
            decision_outcome: DecisionOutcome::Approved,
            enrichment_status: EnrichmentStatus::StructuredOnly,
            source: HistoricalEventSource::OpenFda,
        }
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn admin_override_writes_decision_outcome_and_marks_manually_reviewed(pool: PgPool) {
        let (id, _) = upsert_from_openfda(&pool, &sample_insert("NDA800001"), None)
            .await
            .expect("seed");

        let app = router(AppState::for_tests(pool.clone()));
        let request = Request::builder()
            .method("POST")
            .uri(format!("/admin/historical_events/{id}"))
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"decision_outcome":"crl","notes":"Manually added CRL outcome."}"#,
            ))
            .expect("request");

        let response = app.oneshot(request).await.expect("response");
        assert_eq!(response.status(), StatusCode::OK);

        let row = sqlx::query!(
            r#"SELECT decision_outcome, enrichment_status, notes FROM historical_event WHERE id = $1"#,
            id
        )
        .fetch_one(&pool)
        .await
        .expect("row");
        assert_eq!(row.decision_outcome, "crl");
        assert_eq!(row.enrichment_status, "manually_reviewed");
        assert_eq!(row.notes.as_deref(), Some("Manually added CRL outcome."));
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn admin_override_returns_not_found_for_missing_id(pool: PgPool) {
        let app = router(AppState::for_tests(pool));
        let request = Request::builder()
            .method("POST")
            .uri("/admin/historical_events/11111111-1111-4111-8111-111111111111")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"decision_outcome":"crl"}"#))
            .expect("request");
        let response = app.oneshot(request).await.expect("response");
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn admin_override_rejects_empty_payload(pool: PgPool) {
        let (id, _) = upsert_from_openfda(&pool, &sample_insert("NDA800002"), None)
            .await
            .expect("seed");

        let app = router(AppState::for_tests(pool));
        let request = Request::builder()
            .method("POST")
            .uri(format!("/admin/historical_events/{id}"))
            .header("content-type", "application/json")
            .body(Body::from("{}"))
            .expect("request");
        let response = app.oneshot(request).await.expect("response");
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn admin_override_rejects_out_of_vocabulary_indication(pool: PgPool) {
        let (id, _) = upsert_from_openfda(&pool, &sample_insert("NDA800003"), None)
            .await
            .expect("seed");

        let app = router(AppState::for_tests(pool));
        let request = Request::builder()
            .method("POST")
            .uri(format!("/admin/historical_events/{id}"))
            .header("content-type", "application/json")
            .body(Body::from(r#"{"indication_area":"dentistry"}"#))
            .expect("request");
        let response = app.oneshot(request).await.expect("response");
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}
