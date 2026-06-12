ALTER TABLE maco_session_meta ADD COLUMN permission_mode TEXT NOT NULL DEFAULT 'request_approval'
    CHECK (permission_mode IN ('request_approval', 'auto_approve', 'full_access'));
