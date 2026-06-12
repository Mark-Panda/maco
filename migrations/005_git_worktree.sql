ALTER TABLE maco_session_meta ADD COLUMN git_worktree_enabled INTEGER NOT NULL DEFAULT 1;
ALTER TABLE maco_session_meta ADD COLUMN git_branch_prefix TEXT NOT NULL DEFAULT 'maco/agent';
ALTER TABLE maco_session_meta ADD COLUMN git_worktree_path TEXT;
ALTER TABLE maco_session_meta ADD COLUMN git_worktree_branch TEXT;
