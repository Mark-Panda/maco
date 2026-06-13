//! 会话 workspace 与权限模式解析。

use maco_core::{
    AgentPermissionMode, MacoResult, SessionWorkspace, branch_name, resolve_session_workspace,
    workspace_from_cached,
};
use maco_db::SessionMetaRepo;

#[derive(Clone)]
pub struct SessionContextResolver {
    meta: SessionMetaRepo,
}

impl SessionContextResolver {
    pub fn new(meta: SessionMetaRepo) -> Self {
        Self { meta }
    }

    pub async fn workspace(&self, session_id: &str) -> MacoResult<Option<SessionWorkspace>> {
        let meta = self.meta.get(session_id).await?;
        let Some(rec) = meta else {
            return Ok(None);
        };
        let worktree_enabled = rec.git_worktree_enabled != 0;
        let cached_ok = if worktree_enabled && rec.project_root.is_some() {
            let path_valid = rec
                .git_worktree_path
                .as_deref()
                .filter(|p| !p.trim().is_empty())
                .map(std::path::Path::new)
                .is_some_and(|p| p.exists());
            let expected_branch = branch_name(&rec.git_branch_prefix, session_id);
            let branch_ok = rec.git_worktree_branch.as_deref() == Some(expected_branch.as_str());
            path_valid && branch_ok
        } else {
            !worktree_enabled
        };
        if cached_ok
            && let Some(ws) = workspace_from_cached(
                rec.project_root.as_deref(),
                worktree_enabled,
                rec.git_worktree_path.as_deref(),
                rec.git_worktree_branch.as_deref(),
            )?
            && (!worktree_enabled || ws.uses_worktree)
        {
            return Ok(Some(ws));
        }
        let ws = resolve_session_workspace(
            rec.project_root.as_deref(),
            session_id,
            rec.git_worktree_enabled != 0,
            &rec.git_branch_prefix,
        )?;
        if let Some(ref workspace) = ws {
            if workspace.uses_worktree {
                let branch = workspace.worktree_branch.as_deref().unwrap_or("");
                self.meta
                    .update_worktree_state(
                        session_id,
                        &workspace.workspace_root.to_string_lossy(),
                        branch,
                    )
                    .await?;
            } else {
                self.meta.clear_worktree_state(session_id).await?;
            }
        }
        Ok(ws)
    }

    pub async fn permission_mode(&self, session_id: &str) -> AgentPermissionMode {
        self.meta
            .get(session_id)
            .await
            .ok()
            .flatten()
            .map(|m| AgentPermissionMode::parse(&m.permission_mode))
            .unwrap_or_default()
    }
}
