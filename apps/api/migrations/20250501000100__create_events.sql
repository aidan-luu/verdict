CREATE EXTENSION IF NOT EXISTS pgcrypto;

CREATE TABLE events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    title TEXT NOT NULL,
    kind TEXT NOT NULL DEFAULT 'fda_pdufa',
    drug_name TEXT NOT NULL,
    sponsor TEXT NOT NULL,
    indication TEXT NOT NULL,
    decision_date DATE NOT NULL,
    status TEXT NOT NULL DEFAULT 'upcoming' CHECK (status IN ('upcoming', 'resolved', 'voided')),
    source_url TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX events_decision_date_idx ON events (decision_date);
