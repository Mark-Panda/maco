CREATE TABLE IF NOT EXISTS maco_api_tokens (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    token_hash  TEXT NOT NULL UNIQUE,
    scopes      TEXT NOT NULL DEFAULT '["*"]',
    expires_at  TEXT,
    last_used_at TEXT,
    enabled     INTEGER NOT NULL DEFAULT 1,
    created_at  TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS maco_tool_policies (
    id           TEXT PRIMARY KEY,
    tool_pattern TEXT NOT NULL,
    source_type  TEXT NOT NULL,
    action       TEXT NOT NULL DEFAULT 'confirm'
                 CHECK (action IN ('allow', 'deny', 'confirm')),
    enabled      INTEGER NOT NULL DEFAULT 1,
    created_at   TEXT NOT NULL,
    UNIQUE (tool_pattern, source_type)
);

CREATE TABLE IF NOT EXISTS maco_usage_stats (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id        TEXT REFERENCES maco_session_meta(session_id) ON DELETE SET NULL,
    run_id            TEXT,
    model_id          TEXT REFERENCES maco_models(id) ON DELETE SET NULL,
    model_name        TEXT NOT NULL,
    prompt_tokens     INTEGER NOT NULL DEFAULT 0,
    completion_tokens INTEGER NOT NULL DEFAULT 0,
    total_tokens      INTEGER NOT NULL DEFAULT 0,
    estimated_cost    REAL,
    created_at        TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_maco_usage_stats_day ON maco_usage_stats(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_maco_usage_stats_session ON maco_usage_stats(session_id);

CREATE TABLE IF NOT EXISTS maco_elicitation_requests (
    id           TEXT PRIMARY KEY,
    session_id   TEXT NOT NULL REFERENCES maco_session_meta(session_id) ON DELETE CASCADE,
    run_id       TEXT NOT NULL,
    mcp_server   TEXT NOT NULL,
    request_type TEXT NOT NULL,
    payload      TEXT NOT NULL,
    response     TEXT,
    status       TEXT NOT NULL DEFAULT 'pending'
                 CHECK (status IN ('pending', 'submitted', 'expired', 'cancelled')),
    expires_at   TEXT NOT NULL,
    created_at   TEXT NOT NULL,
    responded_at TEXT
);
CREATE INDEX IF NOT EXISTS idx_maco_elicitation_pending
    ON maco_elicitation_requests(session_id, status) WHERE status = 'pending';

CREATE TABLE IF NOT EXISTS maco_jobs (
    id            TEXT PRIMARY KEY,
    name          TEXT NOT NULL,
    job_type      TEXT NOT NULL,
    schedule      TEXT,
    payload       TEXT NOT NULL DEFAULT '{}',
    status        TEXT NOT NULL DEFAULT 'pending'
                  CHECK (status IN ('pending', 'running', 'completed', 'failed', 'cancelled')),
    last_run_at   TEXT,
    next_run_at   TEXT,
    result        TEXT,
    error_message TEXT,
    enabled       INTEGER NOT NULL DEFAULT 1,
    created_at    TEXT NOT NULL,
    updated_at    TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_maco_jobs_next_run
    ON maco_jobs(next_run_at) WHERE enabled = 1 AND status IN ('pending', 'completed');
