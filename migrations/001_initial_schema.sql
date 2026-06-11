PRAGMA journal_mode = WAL;

CREATE TABLE IF NOT EXISTS maco_models (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL UNIQUE,
    provider    TEXT NOT NULL CHECK (provider IN ('openai', 'anthropic')),
    model_id    TEXT NOT NULL,
    base_url    TEXT,
    api_key_env TEXT NOT NULL,
    is_default  INTEGER NOT NULL DEFAULT 0,
    enabled     INTEGER NOT NULL DEFAULT 1,
    config      TEXT NOT NULL DEFAULT '{}',
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_maco_models_default
    ON maco_models(is_default) WHERE is_default = 1 AND enabled = 1;

CREATE TABLE IF NOT EXISTS maco_session_meta (
    session_id  TEXT PRIMARY KEY,
    title       TEXT,
    model_id    TEXT REFERENCES maco_models(id) ON DELETE SET NULL,
    project_id  TEXT,
    status      TEXT NOT NULL DEFAULT 'active'
                CHECK (status IN ('active', 'archived', 'pending_delete', 'deleted', 'orphan_create')),
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS maco_runs (
    id              TEXT PRIMARY KEY,
    session_id      TEXT NOT NULL,
    status          TEXT NOT NULL,
    resume_context  TEXT,
    superseded_by   TEXT,
    error_message   TEXT,
    last_seq        INTEGER NOT NULL DEFAULT 0,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_maco_runs_session_active
    ON maco_runs(session_id) WHERE status = 'running';
CREATE INDEX IF NOT EXISTS idx_maco_runs_session ON maco_runs(session_id, created_at DESC);

CREATE TABLE IF NOT EXISTS maco_react_plans (
    session_id  TEXT PRIMARY KEY REFERENCES maco_session_meta(session_id) ON DELETE CASCADE,
    content     TEXT NOT NULL DEFAULT '',
    version     INTEGER NOT NULL DEFAULT 1,
    updated_at  TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS maco_react_todos (
    id          TEXT PRIMARY KEY,
    session_id  TEXT NOT NULL REFERENCES maco_session_meta(session_id) ON DELETE CASCADE,
    task_key    TEXT NOT NULL,
    title       TEXT NOT NULL,
    status      TEXT NOT NULL DEFAULT 'pending'
                CHECK (status IN ('pending', 'in_progress', 'completed', 'cancelled')),
    sort_order  INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL,
    UNIQUE (session_id, task_key)
);
CREATE INDEX IF NOT EXISTS idx_maco_react_todos_session ON maco_react_todos(session_id, sort_order);

CREATE TABLE IF NOT EXISTS maco_react_todo_items (
    id           TEXT PRIMARY KEY,
    todo_id      TEXT NOT NULL REFERENCES maco_react_todos(id) ON DELETE CASCADE,
    content      TEXT NOT NULL,
    completed    INTEGER NOT NULL DEFAULT 0,
    completed_at TEXT,
    sort_order   INTEGER NOT NULL DEFAULT 0,
    updated_at   TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_maco_react_todo_items_todo ON maco_react_todo_items(todo_id, sort_order);

CREATE TABLE IF NOT EXISTS maco_mcp_servers (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL UNIQUE,
    transport   TEXT NOT NULL CHECK (transport IN ('stdio', 'sse')),
    command     TEXT,
    args        TEXT NOT NULL DEFAULT '[]',
    url         TEXT,
    env         TEXT NOT NULL DEFAULT '{}',
    enabled     INTEGER NOT NULL DEFAULT 1,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS maco_skills (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL UNIQUE,
    description TEXT,
    content     TEXT,
    file_path   TEXT,
    enabled     INTEGER NOT NULL DEFAULT 1,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS maco_artifacts (
    id           TEXT PRIMARY KEY,
    session_id   TEXT NOT NULL REFERENCES maco_session_meta(session_id) ON DELETE CASCADE,
    filename     TEXT NOT NULL,
    mime_type    TEXT NOT NULL,
    size_bytes   INTEGER NOT NULL,
    storage_path TEXT NOT NULL,
    checksum     TEXT,
    created_at   TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS maco_callback_logs (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id    TEXT NOT NULL,
    run_id        TEXT NOT NULL,
    span_id       TEXT NOT NULL,
    callback_type TEXT NOT NULL,
    agent_name    TEXT,
    model_name    TEXT,
    tool_name     TEXT,
    source_type   TEXT,
    input         TEXT,
    output        TEXT,
    duration_ms   INTEGER,
    status        TEXT NOT NULL DEFAULT 'started',
    error_message TEXT,
    created_at    TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_maco_callback_logs_run ON maco_callback_logs(run_id, created_at);

CREATE TABLE IF NOT EXISTS maco_app_settings (
    key         TEXT PRIMARY KEY,
    value       TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);
