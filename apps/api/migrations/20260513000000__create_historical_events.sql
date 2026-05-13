-- Historical FDA drug-approval decisions ingested from openFDA
-- (drug/drugsfda) and optionally enriched via LLM or manual review.
-- See docs/plans/phase-3.md PR A and SPEC.md data model.

CREATE TABLE IF NOT EXISTS historical_event (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- e.g. "NDA022264" / "BLA761306"; natural key for idempotent upsert.
    application_number TEXT NOT NULL UNIQUE,

    drug_name TEXT NOT NULL,
    sponsor_name TEXT NOT NULL,

    application_type TEXT NOT NULL CHECK (
        application_type IN ('NDA', 'BLA', 'ANDA', 'other')
    ),

    -- Date of the original approved submission (submission_type = 'ORIG',
    -- submission_status = 'AP'), parsed from openFDA's submission_status_date.
    approval_date DATE NOT NULL,

    review_priority TEXT CHECK (
        review_priority IS NULL OR review_priority IN ('priority', 'standard')
    ),

    indication_area TEXT CHECK (
        indication_area IS NULL OR indication_area IN (
            'oncology',
            'metabolic',
            'neurological',
            'cardiovascular',
            'infectious_disease',
            'immunology',
            'rare_disease',
            'other'
        )
    ),

    -- Controlled vocabulary enforced at write time in Rust (services/openfda.rs).
    -- Left as plain TEXT here to keep the vocabulary evolvable without a migration.
    primary_endpoint_type TEXT,

    advisory_committee_held BOOLEAN,

    advisory_committee_vote TEXT CHECK (
        advisory_committee_vote IS NULL
        OR advisory_committee_vote IN ('favorable', 'mixed', 'unfavorable')
    ),

    -- openFDA-sourced rows are always 'approved'; 'crl' / 'approved_with_rems'
    -- are reachable only via the manual override path (PR A) or future sources.
    decision_outcome TEXT NOT NULL CHECK (
        decision_outcome IN ('approved', 'approved_with_rems', 'crl')
    ),

    enrichment_status TEXT NOT NULL CHECK (
        enrichment_status IN ('structured_only', 'llm_enriched', 'manually_reviewed')
    ),

    source TEXT NOT NULL CHECK (source IN ('openfda', 'manual')),

    -- Preserves the original openFDA response so we can re-process when the
    -- mapper or enrichment vocabulary evolves.
    raw_openfda_data JSONB,

    notes TEXT,

    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS historical_event_indication_area_idx
    ON historical_event (indication_area);
CREATE INDEX IF NOT EXISTS historical_event_enrichment_status_idx
    ON historical_event (enrichment_status);
CREATE INDEX IF NOT EXISTS historical_event_approval_date_idx
    ON historical_event (approval_date);
