//! Phase 3 PR B: `GET /events/{id}/reference_class` returns the K most
//! similar enriched `historical_event` rows for a current event, plus
//! aggregate stats with base-rate gating that honestly handles the
//! openFDA approvals-only bias.

use axum::{
    extract::{Path, Query, State},
    Json,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

use crate::services::reference_class::{
    load_enriched_historical_events, match_reference_class, AggregateStats, BaseRateAbsenceReason,
    CurrentEventFeatures, MatchReason, ReferenceClassHit, DEFAULT_K,
};
use crate::{error::AppError, state::AppState};

#[derive(Debug, Deserialize, Validate)]
pub struct ReferenceClassQuery {
    /// Top-K cap on returned hits. Defaults to `DEFAULT_K`. Clamped to a
    /// sane upper bound to keep response sizes reasonable.
    #[validate(range(min = 1, max = 100))]
    pub k: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct ReferenceClassResponse {
    pub event_id: Uuid,
    pub current_features: CurrentEventFeaturesView,
    pub matches: Vec<ReferenceClassHitView>,
    pub aggregate: AggregateStatsView,
}

#[derive(Debug, Serialize)]
pub struct CurrentEventFeaturesView {
    pub indication_area: Option<String>,
    pub application_type: Option<String>,
    pub primary_endpoint_type: Option<String>,
    pub advisory_committee_held: Option<bool>,
    /// `true` if the event has no comparable controlled-vocab features
    /// populated. The matcher will return zero hits in that case; the
    /// frontend uses this flag to show an "enrich this event first"
    /// message instead of a generic empty state.
    pub has_any_feature: bool,
}

#[derive(Debug, Serialize)]
pub struct ReferenceClassHitView {
    pub historical_event_id: Uuid,
    pub application_number: String,
    pub drug_name: String,
    pub sponsor_name: String,
    pub application_type: String,
    pub approval_date: chrono::NaiveDate,
    pub indication_area: Option<String>,
    pub primary_endpoint_type: Option<String>,
    pub advisory_committee_held: Option<bool>,
    pub advisory_committee_vote: Option<String>,
    pub decision_outcome: String,
    pub enrichment_status: String,
    /// Final similarity in `[0, 1]` after recency boost.
    pub similarity_score: f64,
    pub match_reasons: Vec<MatchReason>,
}

#[derive(Debug, Serialize)]
pub struct AggregateStatsView {
    pub sample_size: u32,
    pub approval_count: u32,
    pub crl_count: u32,
    /// Approval rate. `None` when the sample is too thin or contains no
    /// CRLs (see `base_rate_absence_reason`).
    pub base_rate: Option<f64>,
    pub base_rate_absence_reason: Option<BaseRateAbsenceReason>,
    pub enrichment_coverage_pct: u8,
}

pub async fn reference_class_handler(
    State(state): State<AppState>,
    Path(event_id): Path<Uuid>,
    Query(query): Query<ReferenceClassQuery>,
) -> Result<Json<ReferenceClassResponse>, AppError> {
    query.validate()?;

    let event_row = sqlx::query!(
        r#"
        SELECT
            id,
            indication_area,
            application_type,
            primary_endpoint_type,
            advisory_committee_held
        FROM events
        WHERE id = $1
        "#,
        event_id
    )
    .fetch_optional(&state.pool)
    .await?;

    let Some(event_row) = event_row else {
        return Err(AppError::NotFound("event not found".to_string()));
    };

    let features = CurrentEventFeatures {
        indication_area: event_row.indication_area.clone(),
        application_type: event_row.application_type.clone(),
        primary_endpoint_type: event_row.primary_endpoint_type.clone(),
        advisory_committee_held: event_row.advisory_committee_held,
    };
    let has_any_feature = features.indication_area.is_some()
        || features.application_type.is_some()
        || features.primary_endpoint_type.is_some()
        || features.advisory_committee_held.is_some();

    let k = query.k.unwrap_or(DEFAULT_K);
    let historical = load_enriched_historical_events(&state.pool).await?;
    let today = Utc::now().date_naive();
    let result = match_reference_class(&features, historical, k, today);

    let response = ReferenceClassResponse {
        event_id: event_row.id,
        current_features: CurrentEventFeaturesView {
            indication_area: event_row.indication_area,
            application_type: event_row.application_type,
            primary_endpoint_type: event_row.primary_endpoint_type,
            advisory_committee_held: event_row.advisory_committee_held,
            has_any_feature,
        },
        matches: result.top_k.into_iter().map(into_hit_view).collect(),
        aggregate: into_aggregate_view(result.aggregate),
    };

    Ok(Json(response))
}

fn into_hit_view(hit: ReferenceClassHit) -> ReferenceClassHitView {
    ReferenceClassHitView {
        historical_event_id: hit.historical_event.id,
        application_number: hit.historical_event.application_number,
        drug_name: hit.historical_event.drug_name,
        sponsor_name: hit.historical_event.sponsor_name,
        application_type: hit.historical_event.application_type,
        approval_date: hit.historical_event.approval_date,
        indication_area: hit.historical_event.indication_area,
        primary_endpoint_type: hit.historical_event.primary_endpoint_type,
        advisory_committee_held: hit.historical_event.advisory_committee_held,
        advisory_committee_vote: hit.historical_event.advisory_committee_vote,
        decision_outcome: hit.historical_event.decision_outcome,
        enrichment_status: hit.historical_event.enrichment_status,
        similarity_score: hit.similarity_score,
        match_reasons: hit.match_reasons,
    }
}

fn into_aggregate_view(stats: AggregateStats) -> AggregateStatsView {
    AggregateStatsView {
        sample_size: stats.sample_size,
        approval_count: stats.approval_count,
        crl_count: stats.crl_count,
        base_rate: stats.base_rate,
        base_rate_absence_reason: stats.base_rate_absence_reason,
        enrichment_coverage_pct: stats.enrichment_coverage_pct,
    }
}

#[cfg(test)]
mod tests {
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use serde_json::Value;
    use sqlx::PgPool;
    use tower::util::ServiceExt;

    use crate::{app::router, state::AppState};

    /// Seed a small dataset spanning indication_area=oncology and
    /// cardiovascular, with a mix of approved and CRL outcomes, and run
    /// the reference-class route against an oncology event with five
    /// approved + five CRL matches.
    #[sqlx::test(migrations = "../../migrations")]
    async fn reference_class_returns_matches_and_base_rate(pool: PgPool) {
        let event_id = sqlx::query!(
            r#"
            INSERT INTO events (
                title, kind, drug_name, sponsor, indication, decision_date, status,
                indication_area, application_type, primary_endpoint_type, advisory_committee_held
            )
            VALUES (
                'Onco PDUFA', 'fda_pdufa', 'OncoDrug', 'OncoSponsor', 'Onco', '2026-12-01', 'upcoming',
                'oncology', 'NDA', 'overall_survival', true
            )
            RETURNING id
            "#
        )
        .fetch_one(&pool)
        .await
        .expect("event seed")
        .id;

        // Seed: 6 approved oncology rows + 5 CRL oncology rows (all enriched)
        // plus 3 cardiovascular rows the matcher should ignore.
        for i in 0..6u32 {
            let app_no = format!("NDA10{i:04}");
            sqlx::query!(
                r#"
                INSERT INTO historical_event (
                    application_number, drug_name, sponsor_name, application_type,
                    approval_date, decision_outcome, enrichment_status, source,
                    indication_area, primary_endpoint_type, advisory_committee_held
                )
                VALUES (
                    $1, $2, 'TestSponsor', 'NDA',
                    '2024-01-01', 'approved', 'llm_enriched', 'openfda',
                    'oncology', 'overall_survival', true
                )
                "#,
                app_no,
                format!("OncoApproved{i}"),
            )
            .execute(&pool)
            .await
            .expect("approved seed");
        }

        for i in 0..5u32 {
            let app_no = format!("NDA20{i:04}");
            sqlx::query!(
                r#"
                INSERT INTO historical_event (
                    application_number, drug_name, sponsor_name, application_type,
                    approval_date, decision_outcome, enrichment_status, source,
                    indication_area, primary_endpoint_type, advisory_committee_held
                )
                VALUES (
                    $1, $2, 'TestSponsor', 'NDA',
                    '2023-01-01', 'crl', 'manually_reviewed', 'manual',
                    'oncology', 'overall_survival', true
                )
                "#,
                app_no,
                format!("OncoCrl{i}"),
            )
            .execute(&pool)
            .await
            .expect("crl seed");
        }

        for i in 0..3u32 {
            let app_no = format!("NDA30{i:04}");
            sqlx::query!(
                r#"
                INSERT INTO historical_event (
                    application_number, drug_name, sponsor_name, application_type,
                    approval_date, decision_outcome, enrichment_status, source,
                    indication_area, primary_endpoint_type, advisory_committee_held
                )
                VALUES (
                    $1, $2, 'TestSponsor', 'BLA',
                    '2024-01-01', 'approved', 'llm_enriched', 'openfda',
                    'cardiovascular', 'biomarker', false
                )
                "#,
                app_no,
                format!("CardioApproved{i}"),
            )
            .execute(&pool)
            .await
            .expect("cardio seed");
        }

        let app = router(AppState::for_tests(pool));
        let request = Request::builder()
            .uri(format!("/events/{event_id}/reference_class?k=20"))
            .body(Body::empty())
            .expect("request");

        let response = app.oneshot(request).await.expect("run");
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = http_body_util::BodyExt::collect(response.into_body())
            .await
            .expect("collect")
            .to_bytes();
        let body: Value = serde_json::from_slice(&bytes).expect("json");

        let aggregate = &body["aggregate"];
        assert_eq!(aggregate["sample_size"].as_u64(), Some(11));
        assert_eq!(aggregate["approval_count"].as_u64(), Some(6));
        assert_eq!(aggregate["crl_count"].as_u64(), Some(5));
        // 6 / 11 ≈ 0.5454
        let rate = aggregate["base_rate"].as_f64().expect("base_rate");
        assert!((rate - 6.0 / 11.0).abs() < 1e-9, "got {rate}");
        assert!(aggregate["base_rate_absence_reason"].is_null());
        assert_eq!(aggregate["enrichment_coverage_pct"].as_u64(), Some(100));

        let matches = body["matches"].as_array().expect("matches array");
        assert_eq!(matches.len(), 11);
        // Cardio rows should not appear.
        for hit in matches {
            assert_eq!(hit["indication_area"].as_str(), Some("oncology"));
        }
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn reference_class_reports_approval_only_bias_when_no_crls(pool: PgPool) {
        let event_id = sqlx::query!(
            r#"
            INSERT INTO events (
                title, kind, drug_name, sponsor, indication, decision_date, status,
                indication_area
            )
            VALUES ('Onco', 'fda_pdufa', 'Drug', 'Sponsor', 'Onco', '2026-12-01', 'upcoming', 'oncology')
            RETURNING id
            "#
        )
        .fetch_one(&pool)
        .await
        .expect("seed")
        .id;

        for i in 0..7u32 {
            let app_no = format!("NDA40{i:04}");
            sqlx::query!(
                r#"
                INSERT INTO historical_event (
                    application_number, drug_name, sponsor_name, application_type,
                    approval_date, decision_outcome, enrichment_status, source,
                    indication_area
                )
                VALUES (
                    $1, $2, 'S', 'NDA', '2024-01-01', 'approved', 'llm_enriched', 'openfda',
                    'oncology'
                )
                "#,
                app_no,
                format!("OncoApproved{i}"),
            )
            .execute(&pool)
            .await
            .expect("seed approved");
        }

        let app = router(AppState::for_tests(pool));
        let request = Request::builder()
            .uri(format!("/events/{event_id}/reference_class"))
            .body(Body::empty())
            .expect("req");
        let response = app.oneshot(request).await.expect("run");
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = http_body_util::BodyExt::collect(response.into_body())
            .await
            .expect("collect")
            .to_bytes();
        let body: Value = serde_json::from_slice(&bytes).expect("json");
        assert!(body["aggregate"]["base_rate"].is_null());
        assert_eq!(
            body["aggregate"]["base_rate_absence_reason"].as_str(),
            Some("approval_only_bias")
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn reference_class_returns_not_found_for_unknown_event(pool: PgPool) {
        let app = router(AppState::for_tests(pool));
        let request = Request::builder()
            .uri("/events/11111111-1111-4111-8111-111111111111/reference_class")
            .body(Body::empty())
            .expect("req");
        let response = app.oneshot(request).await.expect("run");
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn reference_class_signals_no_features_when_event_unenriched(pool: PgPool) {
        let event_id = sqlx::query!(
            r#"
            INSERT INTO events (title, kind, drug_name, sponsor, indication, decision_date, status)
            VALUES ('A', 'fda_pdufa', 'D', 'S', 'I', '2026-12-01', 'upcoming')
            RETURNING id
            "#
        )
        .fetch_one(&pool)
        .await
        .expect("seed")
        .id;

        let app = router(AppState::for_tests(pool));
        let request = Request::builder()
            .uri(format!("/events/{event_id}/reference_class"))
            .body(Body::empty())
            .expect("req");
        let response = app.oneshot(request).await.expect("run");
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = http_body_util::BodyExt::collect(response.into_body())
            .await
            .expect("collect")
            .to_bytes();
        let body: Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(
            body["current_features"]["has_any_feature"].as_bool(),
            Some(false)
        );
        let matches = body["matches"].as_array().expect("array");
        assert!(matches.is_empty());
    }
}
