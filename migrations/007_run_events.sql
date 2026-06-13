-- Durable SSE event log for Run stream replay.

CREATE TABLE IF NOT EXISTS maco_run_events (
    run_id      TEXT NOT NULL,
    seq         INTEGER NOT NULL,
    event_type  TEXT NOT NULL,
    payload     TEXT NOT NULL,
    created_at  TEXT NOT NULL,
    PRIMARY KEY (run_id, seq),
    FOREIGN KEY (run_id) REFERENCES maco_runs(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_maco_run_events_run_seq
    ON maco_run_events(run_id, seq);
