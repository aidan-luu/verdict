ALTER TABLE events
ADD COLUMN outcome TEXT CHECK (outcome IN ('approved', 'rejected')),
ADD COLUMN resolved_at TIMESTAMPTZ;
