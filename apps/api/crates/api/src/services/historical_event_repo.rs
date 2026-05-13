//! Persistence for `historical_event` rows: upsert on the openFDA path,
//! batch read + per-record enrichment update for the enrichment binary,
//! and admin overrides. Wraps all SQL behind small helpers so binaries
//! and routes never write raw queries.

use serde_json::Value as JsonValue;
use sqlx::PgPool;
use uuid::Uuid;

use crate::services::openfda::{
    DecisionOutcome, EnrichmentStatus, EnrichmentUpdate, HistoricalEventInsert,
    HistoricalEventSource,
};

/// What happened when we upserted a row: `Inserted` when this is a brand-new
/// `application_number`, `Updated` when we found an existing one and refreshed
/// structured fields without clobbering enriched values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpsertOutcome {
    Inserted,
    Updated,
}

/// Insert a new row or refresh the structured fields of an existing one.
/// Enrichment fields (`indication_area`, `primary_endpoint_type`,
/// `advisory_committee_*`) are deliberately **not** touched on conflict, so
/// re-ingestion is safe for already-enriched records.
pub async fn upsert_from_openfda(
    pool: &PgPool,
    record: &HistoricalEventInsert,
    raw: Option<&JsonValue>,
) -> Result<(Uuid, UpsertOutcome), sqlx::Error> {
    // `xmax = 0` after `INSERT ... ON CONFLICT DO UPDATE` is the standard
    // Postgres way to tell apart a fresh insert from an update: the row
    // version's xmax is zero on insert, non-zero after an update.
    let row = sqlx::query!(
        r#"
        INSERT INTO historical_event (
            application_number,
            drug_name,
            sponsor_name,
            application_type,
            approval_date,
            review_priority,
            decision_outcome,
            enrichment_status,
            source,
            raw_openfda_data
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
        ON CONFLICT (application_number) DO UPDATE SET
            drug_name = EXCLUDED.drug_name,
            sponsor_name = EXCLUDED.sponsor_name,
            application_type = EXCLUDED.application_type,
            approval_date = EXCLUDED.approval_date,
            review_priority = EXCLUDED.review_priority,
            raw_openfda_data = EXCLUDED.raw_openfda_data,
            updated_at = now()
        RETURNING id, (xmax = 0) AS "inserted!"
        "#,
        record.application_number,
        record.drug_name,
        record.sponsor_name,
        record.application_type.as_db_str(),
        record.approval_date,
        record.review_priority.map(|priority| priority.as_db_str()),
        record.decision_outcome.as_db_str(),
        record.enrichment_status.as_db_str(),
        record.source.as_db_str(),
        raw,
    )
    .fetch_one(pool)
    .await?;

    let outcome = if row.inserted {
        UpsertOutcome::Inserted
    } else {
        UpsertOutcome::Updated
    };
    Ok((row.id, outcome))
}

/// One row from the batch-read query used by the enrichment binary.
#[derive(Debug, Clone)]
pub struct HistoricalEventForEnrichment {
    pub id: Uuid,
    pub application_number: String,
    pub drug_name: String,
    pub sponsor_name: String,
}

/// Fetch up to `batch_size` rows currently at `enrichment_status =
/// 'structured_only'`, optionally narrowed by approval year and sponsor.
pub async fn fetch_structured_only_batch(
    pool: &PgPool,
    batch_size: i64,
    from_year: Option<i32>,
    sponsor: Option<&str>,
) -> Result<Vec<HistoricalEventForEnrichment>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"
        SELECT id, application_number, drug_name, sponsor_name
        FROM historical_event
        WHERE enrichment_status = 'structured_only'
          AND ($2::int IS NULL OR EXTRACT(YEAR FROM approval_date)::int >= $2)
          AND ($3::text IS NULL OR sponsor_name ILIKE $3)
        ORDER BY approval_date DESC
        LIMIT $1
        "#,
        batch_size,
        from_year,
        sponsor,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| HistoricalEventForEnrichment {
            id: row.id,
            application_number: row.application_number,
            drug_name: row.drug_name,
            sponsor_name: row.sponsor_name,
        })
        .collect())
}

/// Apply an LLM enrichment update. Each field is `Option<...>`; fields
/// passed as `None` are left untouched (so partial enrichment, where the
/// LLM only nailed `indication_area` and nothing else, still writes one
/// column). Promotes `enrichment_status` to `'llm_enriched'` only when at
/// least one field was successfully updated.
pub async fn apply_enrichment(
    pool: &PgPool,
    id: Uuid,
    update: &EnrichmentUpdate,
) -> Result<bool, sqlx::Error> {
    if !update.any_field_present() {
        return Ok(false);
    }

    let result = sqlx::query!(
        r#"
        UPDATE historical_event SET
            indication_area = COALESCE($2, indication_area),
            primary_endpoint_type = COALESCE($3, primary_endpoint_type),
            advisory_committee_held = COALESCE($4, advisory_committee_held),
            advisory_committee_vote = COALESCE($5, advisory_committee_vote),
            enrichment_status = CASE
                WHEN enrichment_status = 'manually_reviewed' THEN 'manually_reviewed'
                ELSE 'llm_enriched'
            END,
            updated_at = now()
        WHERE id = $1
        "#,
        id,
        update.indication_area.as_deref(),
        update.primary_endpoint_type.as_deref(),
        update.advisory_committee_held,
        update.advisory_committee_vote.as_deref(),
    )
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

/// Manual-override payload accepted by `POST /admin/historical_events/{id}`.
/// All fields optional: only fields present in the JSON are applied.
#[derive(Debug, Clone, Default)]
pub struct ManualOverride {
    pub drug_name: Option<String>,
    pub sponsor_name: Option<String>,
    pub indication_area: Option<String>,
    pub primary_endpoint_type: Option<String>,
    pub advisory_committee_held: Option<bool>,
    pub advisory_committee_vote: Option<String>,
    pub decision_outcome: Option<DecisionOutcome>,
    pub notes: Option<String>,
}

/// Apply a manual override. Flips `enrichment_status` to
/// `'manually_reviewed'` unconditionally — manual review is the trump card.
pub async fn apply_manual_override(
    pool: &PgPool,
    id: Uuid,
    override_values: &ManualOverride,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query!(
        r#"
        UPDATE historical_event SET
            drug_name = COALESCE($2, drug_name),
            sponsor_name = COALESCE($3, sponsor_name),
            indication_area = COALESCE($4, indication_area),
            primary_endpoint_type = COALESCE($5, primary_endpoint_type),
            advisory_committee_held = COALESCE($6, advisory_committee_held),
            advisory_committee_vote = COALESCE($7, advisory_committee_vote),
            decision_outcome = COALESCE($8, decision_outcome),
            notes = COALESCE($9, notes),
            enrichment_status = 'manually_reviewed',
            updated_at = now()
        WHERE id = $1
        "#,
        id,
        override_values.drug_name.as_deref(),
        override_values.sponsor_name.as_deref(),
        override_values.indication_area.as_deref(),
        override_values.primary_endpoint_type.as_deref(),
        override_values.advisory_committee_held,
        override_values.advisory_committee_vote.as_deref(),
        override_values
            .decision_outcome
            .map(|outcome| outcome.as_db_str()),
        override_values.notes.as_deref(),
    )
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

/// Insert a brand-new manually-curated record (e.g. a CRL outcome that
/// openFDA does not cover). Returns the new row's id.
pub async fn insert_manual_record(
    pool: &PgPool,
    record: &HistoricalEventInsert,
) -> Result<Uuid, sqlx::Error> {
    debug_assert!(matches!(record.source, HistoricalEventSource::Manual));
    debug_assert!(matches!(
        record.enrichment_status,
        EnrichmentStatus::ManuallyReviewed
    ));

    let row = sqlx::query!(
        r#"
        INSERT INTO historical_event (
            application_number,
            drug_name,
            sponsor_name,
            application_type,
            approval_date,
            review_priority,
            decision_outcome,
            enrichment_status,
            source
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        RETURNING id
        "#,
        record.application_number,
        record.drug_name,
        record.sponsor_name,
        record.application_type.as_db_str(),
        record.approval_date,
        record.review_priority.map(|priority| priority.as_db_str()),
        record.decision_outcome.as_db_str(),
        record.enrichment_status.as_db_str(),
        record.source.as_db_str(),
    )
    .fetch_one(pool)
    .await?;

    Ok(row.id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::openfda::{
        ApplicationType, DecisionOutcome, EnrichmentStatus, HistoricalEventInsert,
        HistoricalEventSource, ReviewPriority,
    };
    use chrono::NaiveDate;
    use serde_json::json;
    use sqlx::PgPool;

    fn sample_insert(application_number: &str) -> HistoricalEventInsert {
        HistoricalEventInsert {
            application_number: application_number.to_string(),
            drug_name: "ACMEDRUG".to_string(),
            sponsor_name: "Acme Pharma".to_string(),
            application_type: ApplicationType::Nda,
            approval_date: NaiveDate::from_ymd_opt(2018, 6, 1).expect("date"),
            review_priority: Some(ReviewPriority::Standard),
            decision_outcome: DecisionOutcome::Approved,
            enrichment_status: EnrichmentStatus::StructuredOnly,
            source: HistoricalEventSource::OpenFda,
        }
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn upsert_is_idempotent_on_application_number(pool: PgPool) {
        let record = sample_insert("NDA900001");
        let raw = json!({"application_number": "NDA900001"});

        let (_id_first, outcome_first) = upsert_from_openfda(&pool, &record, Some(&raw))
            .await
            .expect("first");
        assert_eq!(outcome_first, UpsertOutcome::Inserted);

        let (_id_second, outcome_second) = upsert_from_openfda(&pool, &record, Some(&raw))
            .await
            .expect("second");
        assert_eq!(outcome_second, UpsertOutcome::Updated);

        let count = sqlx::query_scalar!(
            r#"SELECT COUNT(*)::bigint AS "count!" FROM historical_event WHERE application_number = $1"#,
            "NDA900001"
        )
        .fetch_one(&pool)
        .await
        .expect("count");
        assert_eq!(count, 1);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn upsert_does_not_clobber_enriched_fields(pool: PgPool) {
        let record = sample_insert("NDA900002");

        upsert_from_openfda(&pool, &record, None)
            .await
            .expect("first");

        // Simulate a prior enrichment pass having promoted the row.
        sqlx::query!(
            r#"
            UPDATE historical_event SET
                indication_area = 'oncology',
                primary_endpoint_type = 'overall_survival',
                advisory_committee_held = true,
                enrichment_status = 'llm_enriched'
            WHERE application_number = $1
            "#,
            "NDA900002"
        )
        .execute(&pool)
        .await
        .expect("enrich");

        // Re-ingest with the same structured fields. Enrichment must
        // survive untouched.
        upsert_from_openfda(&pool, &record, None)
            .await
            .expect("second");

        let row = sqlx::query!(
            r#"
            SELECT indication_area, primary_endpoint_type, advisory_committee_held,
                   enrichment_status
            FROM historical_event WHERE application_number = $1
            "#,
            "NDA900002"
        )
        .fetch_one(&pool)
        .await
        .expect("row");

        assert_eq!(row.indication_area.as_deref(), Some("oncology"));
        assert_eq!(
            row.primary_endpoint_type.as_deref(),
            Some("overall_survival")
        );
        assert_eq!(row.advisory_committee_held, Some(true));
        assert_eq!(row.enrichment_status, "llm_enriched");
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn apply_enrichment_promotes_status_when_field_written(pool: PgPool) {
        let record = sample_insert("NDA900003");
        let (id, _) = upsert_from_openfda(&pool, &record, None)
            .await
            .expect("seed");

        let update = EnrichmentUpdate {
            indication_area: Some("metabolic".to_string()),
            ..Default::default()
        };
        let changed = apply_enrichment(&pool, id, &update).await.expect("apply");
        assert!(changed);

        let row = sqlx::query!(
            r#"
            SELECT indication_area, enrichment_status
            FROM historical_event WHERE id = $1
            "#,
            id
        )
        .fetch_one(&pool)
        .await
        .expect("row");

        assert_eq!(row.indication_area.as_deref(), Some("metabolic"));
        assert_eq!(row.enrichment_status, "llm_enriched");
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn apply_enrichment_preserves_manually_reviewed_status(pool: PgPool) {
        let record = sample_insert("NDA900004");
        let (id, _) = upsert_from_openfda(&pool, &record, None)
            .await
            .expect("seed");

        sqlx::query!(
            r#"UPDATE historical_event SET enrichment_status = 'manually_reviewed' WHERE id = $1"#,
            id
        )
        .execute(&pool)
        .await
        .expect("promote");

        let update = EnrichmentUpdate {
            primary_endpoint_type: Some("safety".to_string()),
            ..Default::default()
        };
        apply_enrichment(&pool, id, &update).await.expect("apply");

        let status: String = sqlx::query_scalar!(
            r#"SELECT enrichment_status FROM historical_event WHERE id = $1"#,
            id
        )
        .fetch_one(&pool)
        .await
        .expect("status");
        assert_eq!(status, "manually_reviewed");
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn apply_manual_override_flips_status_and_writes_decision_outcome(pool: PgPool) {
        let record = sample_insert("NDA900005");
        let (id, _) = upsert_from_openfda(&pool, &record, None)
            .await
            .expect("seed");

        let override_values = ManualOverride {
            decision_outcome: Some(DecisionOutcome::Crl),
            notes: Some("Hand-curated CRL outcome.".to_string()),
            ..Default::default()
        };
        let changed = apply_manual_override(&pool, id, &override_values)
            .await
            .expect("override");
        assert!(changed);

        let row = sqlx::query!(
            r#"SELECT decision_outcome, enrichment_status, notes FROM historical_event WHERE id = $1"#,
            id
        )
        .fetch_one(&pool)
        .await
        .expect("row");
        assert_eq!(row.decision_outcome, "crl");
        assert_eq!(row.enrichment_status, "manually_reviewed");
        assert_eq!(row.notes.as_deref(), Some("Hand-curated CRL outcome."));
    }
}
