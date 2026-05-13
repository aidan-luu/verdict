//! Reference-class matching for Phase 3 PR B.
//!
//! Given a current `Event`'s feature set, score and rank
//! enriched `historical_event` rows by feature overlap plus a recency
//! boost. Returns the top K hits plus aggregate stats that honestly
//! handle openFDA's approvals-only bias: a base rate is computed only
//! when the matched class contains enough approvals **and** enough CRLs.
//!
//! Math is intentionally simple and inspectable. The plan's note about
//! revisiting embedding similarity is captured below.

// TODO(P3): if the weighted-feature matcher proves too coarse for the
// reference-class panel, revisit embedding-based similarity (indication
// or mechanism vectors). Embeddings add opacity, another model
// dependency, and harder tests, so they are deliberately not the
// starting point.

use std::cmp::Ordering;

use chrono::NaiveDate;
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Weights and tuning constants
// ---------------------------------------------------------------------------

const WEIGHT_INDICATION: f64 = 5.0;
const WEIGHT_APPLICATION_TYPE: f64 = 2.0;
const WEIGHT_PRIMARY_ENDPOINT: f64 = 2.0;
const WEIGHT_ADCOM_HELD: f64 = 1.0;

/// Total weight if every feature comparison succeeded.
const WEIGHT_SUM: f64 =
    WEIGHT_INDICATION + WEIGHT_APPLICATION_TYPE + WEIGHT_PRIMARY_ENDPOINT + WEIGHT_ADCOM_HELD;

/// At zero years old, recency factor is `1.0`. At
/// `RECENCY_HORIZON_YEARS`+ years old, it stays at `RECENCY_FLOOR`.
/// Linear interpolation between those endpoints.
const RECENCY_FLOOR: f64 = 0.5;
const RECENCY_HORIZON_YEARS: f64 = 15.0;

/// Minimum count of approvals **and** CRLs required before we report a
/// base rate. Below this, the panel renders qualitative context only.
pub const BASE_RATE_MIN_PER_SIDE: u32 = 5;

/// Default `k` for top-K matches.
pub const DEFAULT_K: usize = 20;

// ---------------------------------------------------------------------------
// Inputs
// ---------------------------------------------------------------------------

/// Snapshot of a current `Event`'s controlled-vocabulary features used
/// for matching. Free-text fields like `indication` and
/// `primary_endpoint` are deliberately not used here.
#[derive(Debug, Clone, Default)]
pub struct CurrentEventFeatures {
    pub indication_area: Option<String>,
    pub application_type: Option<String>,
    pub primary_endpoint_type: Option<String>,
    pub advisory_committee_held: Option<bool>,
}

/// One enriched `historical_event` row as the matcher sees it. The DB
/// query that produces this should filter out `enrichment_status =
/// 'structured_only'` rows because they lack the comparable features.
#[derive(Debug, Clone)]
pub struct HistoricalEventRow {
    pub id: Uuid,
    pub application_number: String,
    pub drug_name: String,
    pub sponsor_name: String,
    pub application_type: String,
    pub approval_date: NaiveDate,
    pub indication_area: Option<String>,
    pub primary_endpoint_type: Option<String>,
    pub advisory_committee_held: Option<bool>,
    pub advisory_committee_vote: Option<String>,
    pub decision_outcome: String,
    pub enrichment_status: String,
}

// ---------------------------------------------------------------------------
// Outputs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchReason {
    IndicationArea,
    ApplicationType,
    PrimaryEndpointType,
    AdvisoryCommitteeHeld,
}

#[derive(Debug, Clone)]
pub struct ReferenceClassHit {
    pub historical_event: HistoricalEventRow,
    pub similarity_score: f64,
    pub match_reasons: Vec<MatchReason>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BaseRateAbsenceReason {
    /// The matched class has no CRL rows, so the base rate would be
    /// trivially close to 100%. Surface qualitative context instead.
    ApprovalOnlyBias,
    /// Either side has fewer than `BASE_RATE_MIN_PER_SIDE` rows.
    InsufficientSample,
}

#[derive(Debug, Clone)]
pub struct AggregateStats {
    pub sample_size: u32,
    pub approval_count: u32,
    pub crl_count: u32,
    pub base_rate: Option<f64>,
    pub base_rate_absence_reason: Option<BaseRateAbsenceReason>,
    pub enrichment_coverage_pct: u8,
}

#[derive(Debug, Clone)]
pub struct ReferenceClassResult {
    pub top_k: Vec<ReferenceClassHit>,
    pub aggregate: AggregateStats,
}

// ---------------------------------------------------------------------------
// Scoring
// ---------------------------------------------------------------------------

/// Score one (current event, historical row) pair. Returns both the
/// final score and the list of features that actually matched (used by
/// the UI for "matched: indication_area, endpoint_type" chips).
///
/// Score model:
/// - For each feature comparable on **both** sides, add its weight to
///   the raw score if the values match (exact, case-insensitive for
///   strings). Features null on either side are skipped, not penalized.
/// - Normalize by `WEIGHT_SUM` (the total possible if every feature
///   were both present and matched). This is intentional: it means a
///   record with one matched feature scores lower than a record with
///   two matched features, even if the second record also had nullable
///   features that simply weren't compared.
/// - Multiply by a recency factor in `[RECENCY_FLOOR, 1.0]`.
pub fn similarity_score(
    event: &CurrentEventFeatures,
    historical: &HistoricalEventRow,
    today: NaiveDate,
) -> (f64, Vec<MatchReason>) {
    let mut raw = 0.0;
    let mut reasons = Vec::new();

    if let (Some(e), Some(h)) = (
        event.indication_area.as_deref(),
        historical.indication_area.as_deref(),
    ) {
        if e.eq_ignore_ascii_case(h) {
            raw += WEIGHT_INDICATION;
            reasons.push(MatchReason::IndicationArea);
        }
    }

    if let Some(e) = event.application_type.as_deref() {
        if e.eq_ignore_ascii_case(&historical.application_type) {
            raw += WEIGHT_APPLICATION_TYPE;
            reasons.push(MatchReason::ApplicationType);
        }
    }

    if let (Some(e), Some(h)) = (
        event.primary_endpoint_type.as_deref(),
        historical.primary_endpoint_type.as_deref(),
    ) {
        if e.eq_ignore_ascii_case(h) {
            raw += WEIGHT_PRIMARY_ENDPOINT;
            reasons.push(MatchReason::PrimaryEndpointType);
        }
    }

    if let (Some(e), Some(h)) = (
        event.advisory_committee_held,
        historical.advisory_committee_held,
    ) {
        if e == h {
            raw += WEIGHT_ADCOM_HELD;
            reasons.push(MatchReason::AdvisoryCommitteeHeld);
        }
    }

    let normalized = raw / WEIGHT_SUM;
    let recency = recency_factor(years_between(historical.approval_date, today));
    let final_score = normalized * recency;

    (final_score, reasons)
}

fn years_between(start: NaiveDate, end: NaiveDate) -> f64 {
    let days = end.signed_duration_since(start).num_days() as f64;
    // Average year length; precision doesn't need to be calendrical for
    // a recency-only boost.
    days / 365.25
}

pub(crate) fn recency_factor(years_old: f64) -> f64 {
    if years_old <= 0.0 {
        return 1.0;
    }
    if years_old >= RECENCY_HORIZON_YEARS {
        return RECENCY_FLOOR;
    }
    let drop_per_year = (1.0 - RECENCY_FLOOR) / RECENCY_HORIZON_YEARS;
    1.0 - drop_per_year * years_old
}

// ---------------------------------------------------------------------------
// Aggregates
// ---------------------------------------------------------------------------

/// Compute aggregate stats for the matched pool. Base-rate gating
/// enforces the approval-bias caveat from `SPEC.md` and
/// `docs/historical_events_curation.md`.
pub fn compute_aggregates(hits: &[ReferenceClassHit]) -> AggregateStats {
    let sample_size = hits.len() as u32;
    let mut approval_count = 0u32;
    let mut crl_count = 0u32;
    let mut enriched_count = 0u32;

    for hit in hits {
        match hit.historical_event.decision_outcome.as_str() {
            "approved" | "approved_with_rems" => approval_count += 1,
            "crl" => crl_count += 1,
            _ => (),
        }
        if matches!(
            hit.historical_event.enrichment_status.as_str(),
            "llm_enriched" | "manually_reviewed"
        ) {
            enriched_count += 1;
        }
    }

    let (base_rate, absence_reason) =
        if approval_count >= BASE_RATE_MIN_PER_SIDE && crl_count >= BASE_RATE_MIN_PER_SIDE {
            let total = (approval_count + crl_count) as f64;
            (Some(approval_count as f64 / total), None)
        } else if crl_count == 0 && approval_count > 0 {
            (None, Some(BaseRateAbsenceReason::ApprovalOnlyBias))
        } else {
            (None, Some(BaseRateAbsenceReason::InsufficientSample))
        };

    let enrichment_coverage_pct = if sample_size == 0 {
        0
    } else {
        ((enriched_count as f64 / sample_size as f64) * 100.0).round() as u8
    };

    AggregateStats {
        sample_size,
        approval_count,
        crl_count,
        base_rate,
        base_rate_absence_reason: absence_reason,
        enrichment_coverage_pct,
    }
}

// ---------------------------------------------------------------------------
// Top-level entry: rank, truncate, aggregate
// ---------------------------------------------------------------------------

/// Score every supplied historical row, drop zero-match rows, sort by
/// descending similarity, compute aggregates **over the full matched
/// pool** (so the base rate is honest about the reference class even
/// when the panel only renders the first K), and truncate to top K.
pub fn match_reference_class(
    event: &CurrentEventFeatures,
    historical_events: Vec<HistoricalEventRow>,
    k: usize,
    today: NaiveDate,
) -> ReferenceClassResult {
    let mut hits: Vec<ReferenceClassHit> = historical_events
        .into_iter()
        .filter_map(|historical| {
            let (score, reasons) = similarity_score(event, &historical, today);
            if reasons.is_empty() {
                None
            } else {
                Some(ReferenceClassHit {
                    historical_event: historical,
                    similarity_score: score,
                    match_reasons: reasons,
                })
            }
        })
        .collect();

    hits.sort_by(|a, b| {
        b.similarity_score
            .partial_cmp(&a.similarity_score)
            .unwrap_or(Ordering::Equal)
            .then_with(|| {
                b.historical_event
                    .approval_date
                    .cmp(&a.historical_event.approval_date)
            })
    });

    let aggregate = compute_aggregates(&hits);
    let top_k = hits.into_iter().take(k).collect();

    ReferenceClassResult { top_k, aggregate }
}

// ---------------------------------------------------------------------------
// DB loader
// ---------------------------------------------------------------------------

/// Load all enriched `historical_event` rows for matching. Excludes
/// `structured_only` rows because they lack the comparable features.
pub async fn load_enriched_historical_events(
    pool: &PgPool,
) -> Result<Vec<HistoricalEventRow>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"
        SELECT
            id,
            application_number,
            drug_name,
            sponsor_name,
            application_type,
            approval_date,
            indication_area,
            primary_endpoint_type,
            advisory_committee_held,
            advisory_committee_vote,
            decision_outcome,
            enrichment_status
        FROM historical_event
        WHERE enrichment_status <> 'structured_only'
        ORDER BY approval_date DESC
        "#
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| HistoricalEventRow {
            id: row.id,
            application_number: row.application_number,
            drug_name: row.drug_name,
            sponsor_name: row.sponsor_name,
            application_type: row.application_type,
            approval_date: row.approval_date,
            indication_area: row.indication_area,
            primary_endpoint_type: row.primary_endpoint_type,
            advisory_committee_held: row.advisory_committee_held,
            advisory_committee_vote: row.advisory_committee_vote,
            decision_outcome: row.decision_outcome,
            enrichment_status: row.enrichment_status,
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 5, 13).expect("today")
    }

    fn fresh_historical(
        indication_area: Option<&str>,
        application_type: &str,
        primary_endpoint_type: Option<&str>,
        advisory_committee_held: Option<bool>,
        years_old: i64,
        outcome: &str,
        enrichment_status: &str,
    ) -> HistoricalEventRow {
        let approval_date = today() - chrono::Duration::days((years_old * 365) + (years_old / 4));
        HistoricalEventRow {
            id: Uuid::new_v4(),
            application_number: "NDA000001".to_string(),
            drug_name: "TESTDRUG".to_string(),
            sponsor_name: "TestPharma".to_string(),
            application_type: application_type.to_string(),
            approval_date,
            indication_area: indication_area.map(str::to_string),
            primary_endpoint_type: primary_endpoint_type.map(str::to_string),
            advisory_committee_held,
            advisory_committee_vote: None,
            decision_outcome: outcome.to_string(),
            enrichment_status: enrichment_status.to_string(),
        }
    }

    fn event(
        indication_area: Option<&str>,
        application_type: Option<&str>,
        primary_endpoint_type: Option<&str>,
        advisory_committee_held: Option<bool>,
    ) -> CurrentEventFeatures {
        CurrentEventFeatures {
            indication_area: indication_area.map(str::to_string),
            application_type: application_type.map(str::to_string),
            primary_endpoint_type: primary_endpoint_type.map(str::to_string),
            advisory_committee_held,
        }
    }

    // -----------------------------------------------------------------
    // Similarity scoring — hand-computed expected values
    // -----------------------------------------------------------------

    #[test]
    fn indication_only_match_scores_half_at_zero_years_old() {
        // raw = 5, normalized = 5/10 = 0.5, recency = 1.0 -> 0.5
        let evt = event(Some("oncology"), None, None, None);
        let hist = fresh_historical(
            Some("oncology"),
            "NDA",
            None,
            None,
            0,
            "approved",
            "llm_enriched",
        );
        let (score, reasons) = similarity_score(&evt, &hist, today());
        assert!((score - 0.5).abs() < 1e-9, "got {score}");
        assert_eq!(reasons, vec![MatchReason::IndicationArea]);
    }

    #[test]
    fn indication_plus_endpoint_match_scores_higher_than_indication_only() {
        // raw = 5 + 2 = 7, normalized = 7/10 = 0.7, recency = 1.0 -> 0.7
        let evt = event(Some("oncology"), None, Some("overall_survival"), None);
        let hist = fresh_historical(
            Some("oncology"),
            "NDA",
            Some("overall_survival"),
            None,
            0,
            "approved",
            "llm_enriched",
        );
        let (score, reasons) = similarity_score(&evt, &hist, today());
        assert!((score - 0.7).abs() < 1e-9, "got {score}");
        assert_eq!(
            reasons,
            vec![
                MatchReason::IndicationArea,
                MatchReason::PrimaryEndpointType
            ]
        );
    }

    #[test]
    fn null_endpoint_on_historical_does_not_penalize_indication_only_match() {
        // raw = 5, but only indication is compared; normalized 5/10 = 0.5.
        // This intentionally LOWER than the indication+endpoint match
        // above, because we want richer matches to rank higher.
        let evt = event(Some("oncology"), None, Some("overall_survival"), None);
        let hist = fresh_historical(
            Some("oncology"),
            "NDA",
            None,
            None,
            0,
            "approved",
            "llm_enriched",
        );
        let (score, reasons) = similarity_score(&evt, &hist, today());
        assert!((score - 0.5).abs() < 1e-9, "got {score}");
        assert_eq!(reasons, vec![MatchReason::IndicationArea]);
    }

    #[test]
    fn all_features_match_at_zero_years_old_scores_one() {
        // raw = 10, normalized = 1.0, recency = 1.0 -> 1.0
        let evt = event(
            Some("oncology"),
            Some("NDA"),
            Some("overall_survival"),
            Some(true),
        );
        let hist = fresh_historical(
            Some("oncology"),
            "NDA",
            Some("overall_survival"),
            Some(true),
            0,
            "approved",
            "llm_enriched",
        );
        let (score, _) = similarity_score(&evt, &hist, today());
        assert!((score - 1.0).abs() < 1e-9, "got {score}");
    }

    #[test]
    fn no_overlap_yields_zero_score_and_empty_reasons() {
        let evt = event(Some("oncology"), Some("NDA"), None, None);
        let hist = fresh_historical(
            Some("cardiovascular"),
            "BLA",
            None,
            None,
            0,
            "approved",
            "llm_enriched",
        );
        let (score, reasons) = similarity_score(&evt, &hist, today());
        assert_eq!(score, 0.0);
        assert!(reasons.is_empty());
    }

    // -----------------------------------------------------------------
    // Recency boost
    // -----------------------------------------------------------------

    #[test]
    fn recency_factor_endpoints_and_floor() {
        assert!((recency_factor(0.0) - 1.0).abs() < 1e-9);
        // Linear at 15: floor.
        assert!((recency_factor(15.0) - 0.5).abs() < 1e-9);
        // Past horizon clamps to floor.
        assert!((recency_factor(30.0) - 0.5).abs() < 1e-9);
        // At 7.5 years (midpoint), factor = 0.75.
        assert!((recency_factor(7.5) - 0.75).abs() < 1e-9);
    }

    #[test]
    fn recency_factor_monotonic_in_age() {
        let mut prev = recency_factor(0.0);
        for years in [1.0, 3.0, 5.0, 7.5, 10.0, 14.0, 15.0, 25.0] {
            let cur = recency_factor(years);
            assert!(
                cur <= prev + 1e-9,
                "recency must be monotonically non-increasing: {prev} -> {cur} at {years}y"
            );
            prev = cur;
        }
    }

    #[test]
    fn recency_boost_breaks_ties_between_otherwise_equal_matches() {
        let evt = event(Some("oncology"), None, None, None);
        let recent = fresh_historical(
            Some("oncology"),
            "NDA",
            None,
            None,
            2,
            "approved",
            "llm_enriched",
        );
        let older = fresh_historical(
            Some("oncology"),
            "NDA",
            None,
            None,
            10,
            "approved",
            "llm_enriched",
        );
        let (recent_score, _) = similarity_score(&evt, &recent, today());
        let (older_score, _) = similarity_score(&evt, &older, today());
        assert!(
            recent_score > older_score,
            "{recent_score} <= {older_score}"
        );
    }

    // -----------------------------------------------------------------
    // Aggregate / base-rate gating
    // -----------------------------------------------------------------

    fn hit_with_outcome_and_status(outcome: &str, enrichment_status: &str) -> ReferenceClassHit {
        ReferenceClassHit {
            historical_event: fresh_historical(
                Some("oncology"),
                "NDA",
                None,
                None,
                1,
                outcome,
                enrichment_status,
            ),
            similarity_score: 0.5,
            match_reasons: vec![MatchReason::IndicationArea],
        }
    }

    #[test]
    fn base_rate_present_when_five_approvals_and_five_crls() {
        let hits: Vec<ReferenceClassHit> =
            std::iter::repeat_with(|| hit_with_outcome_and_status("approved", "llm_enriched"))
                .take(5)
                .chain(
                    std::iter::repeat_with(|| {
                        hit_with_outcome_and_status("crl", "manually_reviewed")
                    })
                    .take(5),
                )
                .collect();
        let stats = compute_aggregates(&hits);
        assert_eq!(stats.approval_count, 5);
        assert_eq!(stats.crl_count, 5);
        assert_eq!(stats.base_rate, Some(0.5));
        assert!(stats.base_rate_absence_reason.is_none());
        assert_eq!(stats.enrichment_coverage_pct, 100);
    }

    #[test]
    fn base_rate_gated_with_insufficient_sample_when_only_four_crls() {
        let hits: Vec<ReferenceClassHit> =
            std::iter::repeat_with(|| hit_with_outcome_and_status("approved", "llm_enriched"))
                .take(5)
                .chain(
                    std::iter::repeat_with(|| {
                        hit_with_outcome_and_status("crl", "manually_reviewed")
                    })
                    .take(4),
                )
                .collect();
        let stats = compute_aggregates(&hits);
        assert_eq!(stats.base_rate, None);
        assert_eq!(
            stats.base_rate_absence_reason,
            Some(BaseRateAbsenceReason::InsufficientSample)
        );
    }

    #[test]
    fn base_rate_gated_with_approval_only_bias_when_no_crls() {
        let hits: Vec<ReferenceClassHit> =
            std::iter::repeat_with(|| hit_with_outcome_and_status("approved", "llm_enriched"))
                .take(10)
                .collect();
        let stats = compute_aggregates(&hits);
        assert_eq!(stats.approval_count, 10);
        assert_eq!(stats.crl_count, 0);
        assert_eq!(stats.base_rate, None);
        assert_eq!(
            stats.base_rate_absence_reason,
            Some(BaseRateAbsenceReason::ApprovalOnlyBias)
        );
    }

    #[test]
    fn aggregates_count_approved_with_rems_as_approval() {
        let hits = vec![
            hit_with_outcome_and_status("approved", "llm_enriched"),
            hit_with_outcome_and_status("approved_with_rems", "llm_enriched"),
            hit_with_outcome_and_status("crl", "manually_reviewed"),
        ];
        let stats = compute_aggregates(&hits);
        assert_eq!(stats.approval_count, 2);
        assert_eq!(stats.crl_count, 1);
    }

    #[test]
    fn enrichment_coverage_pct_rounds_to_nearest_integer() {
        let hits = vec![
            hit_with_outcome_and_status("approved", "llm_enriched"),
            hit_with_outcome_and_status("approved", "structured_only"),
            hit_with_outcome_and_status("approved", "llm_enriched"),
        ];
        let stats = compute_aggregates(&hits);
        // 2 of 3 enriched = 66.667 -> 67
        assert_eq!(stats.enrichment_coverage_pct, 67);
    }

    // -----------------------------------------------------------------
    // Top-level match_reference_class: sorting + truncation + aggregates
    // -----------------------------------------------------------------

    #[test]
    fn match_reference_class_sorts_by_score_then_recency() {
        let evt = event(Some("oncology"), Some("NDA"), None, None);
        let hist_better = fresh_historical(
            Some("oncology"),
            "NDA",
            None,
            None,
            1,
            "approved",
            "llm_enriched",
        );
        let hist_worse = fresh_historical(
            Some("oncology"),
            "BLA",
            None,
            None,
            1,
            "approved",
            "llm_enriched",
        );
        let result = match_reference_class(&evt, vec![hist_worse, hist_better], 10, today());
        assert_eq!(result.top_k.len(), 2);
        // Indication+app_type match beats indication-only match.
        assert!(result.top_k[0].similarity_score > result.top_k[1].similarity_score);
    }

    #[test]
    fn match_reference_class_drops_unmatched_rows() {
        let evt = event(Some("oncology"), None, None, None);
        let matcher = fresh_historical(
            Some("oncology"),
            "NDA",
            None,
            None,
            1,
            "approved",
            "llm_enriched",
        );
        let unrelated = fresh_historical(
            Some("cardiovascular"),
            "BLA",
            None,
            None,
            1,
            "approved",
            "llm_enriched",
        );
        let result = match_reference_class(&evt, vec![matcher, unrelated], 10, today());
        assert_eq!(result.top_k.len(), 1);
    }

    #[test]
    fn match_reference_class_truncates_to_k_but_aggregates_full_pool() {
        let evt = event(Some("oncology"), None, None, None);
        let mut rows = Vec::new();
        for _ in 0..30 {
            rows.push(fresh_historical(
                Some("oncology"),
                "NDA",
                None,
                None,
                1,
                "approved",
                "llm_enriched",
            ));
        }
        let result = match_reference_class(&evt, rows, 5, today());
        assert_eq!(result.top_k.len(), 5);
        // Aggregate covers all 30 matches, not just top 5.
        assert_eq!(result.aggregate.sample_size, 30);
        assert_eq!(result.aggregate.approval_count, 30);
        // Approval-only -> bias caveat, not a base rate.
        assert!(result.aggregate.base_rate.is_none());
    }
}
