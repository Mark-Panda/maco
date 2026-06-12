//! 会话元数据 API 视图（含计算字段）。

use maco_core::git_worktree_status;
use maco_db::SessionMetaRecord;
use serde::Serialize;

/// `GET/POST /sessions` 返回的会话元数据（含 worktree 状态）。
#[derive(Debug, Clone, Serialize)]
pub struct SessionMetaView {
    #[serde(flatten)]
    pub meta: SessionMetaRecord,
    /// worktree 状态：`disabled` / `no_project` / `not_git_repo` / `git_unavailable` / `pending` / `active`
    pub git_worktree_status: String,
}

impl SessionMetaView {
    pub fn from_record(rec: SessionMetaRecord) -> Self {
        let status = git_worktree_status(
            rec.git_worktree_enabled != 0,
            rec.project_root.as_deref(),
            rec.git_worktree_path.as_deref(),
        );
        Self {
            meta: rec,
            git_worktree_status: status.to_string(),
        }
    }
}

pub fn enrich_sessions(records: Vec<SessionMetaRecord>) -> Vec<SessionMetaView> {
    records.into_iter().map(SessionMetaView::from_record).collect()
}
