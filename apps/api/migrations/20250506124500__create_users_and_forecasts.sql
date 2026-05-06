CREATE TABLE users (
    id UUID PRIMARY KEY,
    clerk_id TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

INSERT INTO users (id, clerk_id)
VALUES ('00000000-0000-4000-8000-000000000001', 'stub-user')
ON CONFLICT (id) DO NOTHING;

CREATE TABLE forecasts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users (id),
    event_id UUID NOT NULL REFERENCES events (id),
    probability NUMERIC(5, 4) NOT NULL CHECK (probability >= 0 AND probability <= 1),
    rationale TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX forecasts_user_event_idx ON forecasts (user_id, event_id);
