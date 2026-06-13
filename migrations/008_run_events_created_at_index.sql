-- Speed up startup retention cleanup for durable Run SSE events.

CREATE INDEX IF NOT EXISTS idx_maco_run_events_created_at
    ON maco_run_events(created_at);
