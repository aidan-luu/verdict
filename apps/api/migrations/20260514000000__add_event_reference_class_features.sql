-- Phase 3 PR B: add controlled-vocabulary feature columns to `events` so the
-- reference-class matcher can compare current events against `historical_event`
-- rows along the same dimensions. All nullable; existing events stay valid.
-- The matcher tolerates nulls on either side.

ALTER TABLE events
ADD COLUMN indication_area TEXT CHECK (
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
ADD COLUMN application_type TEXT CHECK (
    application_type IS NULL OR application_type IN ('NDA', 'BLA', 'ANDA', 'other')
),
-- Plain TEXT; vocabulary enforced in Rust at write time so it can evolve
-- without a migration (mirrors `historical_event.primary_endpoint_type`).
ADD COLUMN primary_endpoint_type TEXT,
ADD COLUMN advisory_committee_held BOOLEAN;

CREATE INDEX events_indication_area_idx ON events (indication_area);
