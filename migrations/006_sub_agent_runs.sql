-- Sub-Agent spawn 审计（ReAct spawn_sub_agent）

CREATE TABLE IF NOT EXISTS maco_sub_agent_runs (
    id              TEXT PRIMARY KEY,
    session_id      TEXT NOT NULL,
    parent_run_id   TEXT NOT NULL,
    task_key        TEXT NOT NULL,
    worker_agent    TEXT NOT NULL,
    tools_profile   TEXT NOT NULL DEFAULT 'coding',
    status          TEXT NOT NULL
                    CHECK (status IN ('running', 'completed', 'failed', 'cancelled', 'timeout')),
    instruction     TEXT NOT NULL,
    summary         TEXT,
    error           TEXT,
    spawn_count     INTEGER NOT NULL DEFAULT 1,
    model_id        TEXT,
    usage_tokens    INTEGER,
    started_at      TEXT NOT NULL,
    finished_at     TEXT,
    FOREIGN KEY (session_id) REFERENCES maco_session_meta(session_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_maco_sub_agent_runs_session
    ON maco_sub_agent_runs(session_id, started_at DESC);

CREATE INDEX IF NOT EXISTS idx_maco_sub_agent_runs_parent
    ON maco_sub_agent_runs(parent_run_id, started_at DESC);

CREATE INDEX IF NOT EXISTS idx_maco_sub_agent_runs_task
    ON maco_sub_agent_runs(session_id, task_key, started_at DESC);
