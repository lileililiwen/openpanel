-- SSL v0.2: track last_attempt_at for the 24h renewal backoff.
-- Adds a nullable TEXT column. Existing rows get NULL (never attempted).

ALTER TABLE certificates ADD COLUMN last_attempt_at TEXT;
