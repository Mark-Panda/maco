//! `maco_session_meta` 表：会话标题、模型绑定与生命周期状态。

use chrono::Utc;
use std::path::Path;

use maco_core::{
    AgentPermissionMode, DEFAULT_GIT_BRANCH_PREFIX, GitRepoProbe, MacoError, MacoResult,
    probe_git_repository, resolve_project_root,
};
use sqlx::SqlitePool;

const SESSION_META_SELECT: &str = "SELECT session_id, title, model_id, project_id, project_root, \
    permission_mode, git_worktree_enabled, git_branch_prefix, git_worktree_path, git_worktree_branch, \
    status, created_at, updated_at FROM maco_session_meta";

/// 业务侧会话元数据（与 adk session_id 一一对应）。
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct SessionMetaRecord {
    /// 会话 ID（与 adk 一致）。
    pub session_id: String,
    /// 显示标题。
    pub title: Option<String>,
    /// 绑定模型 ID。
    pub model_id: Option<String>,
    /// 所属项目 ID（预留）。
    pub project_id: Option<String>,
    /// 绑定的本地项目根目录（绝对路径，Git 仓库根）。
    pub project_root: Option<String>,
    /// Agent 权限模式。
    pub permission_mode: String,
    /// 是否强制使用 Git worktree 编辑代码（SQLite 0/1）。
    pub git_worktree_enabled: i64,
    /// worktree 分支前缀。
    pub git_branch_prefix: String,
    /// 当前 worktree 检出路径。
    pub git_worktree_path: Option<String>,
    /// 当前 worktree 分支名。
    pub git_worktree_branch: Option<String>,
    /// 生命周期状态。
    pub status: String,
    /// 创建时间。
    pub created_at: String,
    /// 最后活动时间。
    pub updated_at: String,
}

#[derive(Clone)]
pub struct SessionMetaRepo {
    pool: SqlitePool,
}

impl SessionMetaRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn insert(&self, rec: &SessionMetaRecord) -> MacoResult<()> {
        sqlx::query(
            "INSERT INTO maco_session_meta (
                session_id, title, model_id, project_id, project_root, permission_mode,
                git_worktree_enabled, git_branch_prefix, git_worktree_path, git_worktree_branch,
                status, created_at, updated_at
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&rec.session_id)
        .bind(&rec.title)
        .bind(&rec.model_id)
        .bind(&rec.project_id)
        .bind(&rec.project_root)
        .bind(&rec.permission_mode)
        .bind(rec.git_worktree_enabled)
        .bind(&rec.git_branch_prefix)
        .bind(&rec.git_worktree_path)
        .bind(&rec.git_worktree_branch)
        .bind(&rec.status)
        .bind(&rec.created_at)
        .bind(&rec.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|e| MacoError::database(e.to_string()))?;
        Ok(())
    }

    pub async fn get(&self, session_id: &str) -> MacoResult<Option<SessionMetaRecord>> {
        let q = format!("{SESSION_META_SELECT} WHERE session_id = ?");
        sqlx::query_as::<_, SessionMetaRecord>(&q)
            .bind(session_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| MacoError::database(e.to_string()))
    }

    pub async fn list_active(&self) -> MacoResult<Vec<SessionMetaRecord>> {
        let q = format!(
            "{SESSION_META_SELECT} WHERE status NOT IN ('deleted') ORDER BY updated_at DESC"
        );
        sqlx::query_as::<_, SessionMetaRecord>(&q)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| MacoError::database(e.to_string()))
    }

    pub async fn list_by_ids(&self, ids: &[String]) -> MacoResult<Vec<SessionMetaRecord>> {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let q = format!("{SESSION_META_SELECT} WHERE session_id IN ({placeholders})");
        let mut query = sqlx::query_as::<_, SessionMetaRecord>(&q);
        for id in ids {
            query = query.bind(id);
        }
        query
            .fetch_all(&self.pool)
            .await
            .map_err(|e| MacoError::database(e.to_string()))
    }

    pub async fn update_status(&self, session_id: &str, status: &str) -> MacoResult<()> {
        let now = Utc::now().to_rfc3339();
        sqlx::query("UPDATE maco_session_meta SET status = ?, updated_at = ? WHERE session_id = ?")
            .bind(status)
            .bind(now)
            .bind(session_id)
            .execute(&self.pool)
            .await
            .map_err(|e| MacoError::database(e.to_string()))?;
        Ok(())
    }

    pub async fn touch(&self, session_id: &str) -> MacoResult<()> {
        let now = Utc::now().to_rfc3339();
        sqlx::query("UPDATE maco_session_meta SET updated_at = ? WHERE session_id = ?")
            .bind(now)
            .bind(session_id)
            .execute(&self.pool)
            .await
            .map_err(|e| MacoError::database(e.to_string()))?;
        Ok(())
    }

    pub async fn update_title_model(
        &self,
        session_id: &str,
        title: Option<&str>,
        model_id: Option<&str>,
    ) -> MacoResult<()> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "UPDATE maco_session_meta SET title = COALESCE(?, title), model_id = COALESCE(?, model_id), updated_at = ? WHERE session_id = ?",
        )
        .bind(title)
        .bind(model_id)
        .bind(now)
        .bind(session_id)
        .execute(&self.pool)
        .await
        .map_err(|e| MacoError::database(e.to_string()))?;
        Ok(())
    }

    /// 单会话 Agent 工作目录（供 filesystem MCP 根目录计算）。
    pub fn agent_workspace_root_from_record(rec: &SessionMetaRecord) -> Option<String> {
        if let Some(path) = rec
            .git_worktree_path
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .filter(|p| Path::new(p).exists())
        {
            return Some(path.to_string());
        }

        let project = rec
            .project_root
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());

        if rec.git_worktree_enabled != 0 {
            project?;
            // Git 仓库且 worktree 待 provision：不把主仓库加入 MCP，Run 内会先 provision。
            if let Ok(Some(repo)) = resolve_project_root(rec.project_root.as_deref())
                && probe_git_repository(&repo) == GitRepoProbe::Available
            {
                return None;
            }
            return project
                .filter(|p| Path::new(p).exists())
                .map(str::to_string);
        }

        project
            .filter(|p| Path::new(p).exists())
            .map(str::to_string)
    }

    /// 查询指定会话的 Agent 工作目录（未 provision worktree 时返回 `None`）。
    pub async fn agent_workspace_root_for_session(
        &self,
        session_id: &str,
    ) -> MacoResult<Option<String>> {
        let rec = self.get(session_id).await?;
        Ok(rec
            .as_ref()
            .and_then(Self::agent_workspace_root_from_record))
    }

    /// 各活跃会话 Agent 工作目录并集（每会话按 worktree 策略单独计算，避免主仓库与 worktree 混用）。
    pub async fn list_distinct_workspace_roots(&self) -> MacoResult<Vec<String>> {
        let mut seen = std::collections::HashSet::new();
        let mut roots = Vec::new();
        for rec in self.list_active().await? {
            if let Some(root) = Self::agent_workspace_root_from_record(&rec)
                && seen.insert(root.clone())
            {
                roots.push(root);
            }
        }
        Ok(roots)
    }

    /// 创建失败回滚时硬删除元数据行。
    pub async fn delete_hard(&self, session_id: &str) -> MacoResult<()> {
        sqlx::query("DELETE FROM maco_session_meta WHERE session_id = ?")
            .bind(session_id)
            .execute(&self.pool)
            .await
            .map_err(|e| MacoError::database(e.to_string()))?;
        Ok(())
    }

    /// 兼容旧调用：等同 `list_distinct_workspace_roots`。
    pub async fn list_distinct_project_roots(&self) -> MacoResult<Vec<String>> {
        self.list_distinct_workspace_roots().await
    }

    pub async fn update_project_root(
        &self,
        session_id: &str,
        project_root: Option<&str>,
    ) -> MacoResult<()> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "UPDATE maco_session_meta SET project_root = ?, git_worktree_path = NULL, git_worktree_branch = NULL, updated_at = ? WHERE session_id = ?",
        )
        .bind(project_root)
        .bind(now)
        .bind(session_id)
        .execute(&self.pool)
        .await
        .map_err(|e| MacoError::database(e.to_string()))?;
        Ok(())
    }

    pub async fn update_permission_mode(
        &self,
        session_id: &str,
        mode: AgentPermissionMode,
    ) -> MacoResult<()> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "UPDATE maco_session_meta SET permission_mode = ?, updated_at = ? WHERE session_id = ?",
        )
        .bind(mode.as_str())
        .bind(now)
        .bind(session_id)
        .execute(&self.pool)
        .await
        .map_err(|e| MacoError::database(e.to_string()))?;
        Ok(())
    }

    pub async fn update_git_worktree_settings(
        &self,
        session_id: &str,
        enabled: bool,
        branch_prefix: &str,
    ) -> MacoResult<()> {
        let now = Utc::now().to_rfc3339();
        let prefix = if branch_prefix.trim().is_empty() {
            DEFAULT_GIT_BRANCH_PREFIX
        } else {
            branch_prefix.trim()
        };
        sqlx::query(
            "UPDATE maco_session_meta SET git_worktree_enabled = ?, git_branch_prefix = ?, updated_at = ? WHERE session_id = ?",
        )
        .bind(if enabled { 1 } else { 0 })
        .bind(prefix)
        .bind(now)
        .bind(session_id)
        .execute(&self.pool)
        .await
        .map_err(|e| MacoError::database(e.to_string()))?;
        Ok(())
    }

    pub async fn update_worktree_state(
        &self,
        session_id: &str,
        worktree_path: &str,
        branch: &str,
    ) -> MacoResult<()> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "UPDATE maco_session_meta SET git_worktree_path = ?, git_worktree_branch = ?, updated_at = ? WHERE session_id = ?",
        )
        .bind(worktree_path)
        .bind(branch)
        .bind(now)
        .bind(session_id)
        .execute(&self.pool)
        .await
        .map_err(|e| MacoError::database(e.to_string()))?;
        Ok(())
    }

    pub async fn clear_worktree_state(&self, session_id: &str) -> MacoResult<()> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "UPDATE maco_session_meta SET git_worktree_path = NULL, git_worktree_branch = NULL, updated_at = ? WHERE session_id = ?",
        )
        .bind(now)
        .bind(session_id)
        .execute(&self.pool)
        .await
        .map_err(|e| MacoError::database(e.to_string()))?;
        Ok(())
    }

    pub async fn list_orphans(&self) -> MacoResult<Vec<SessionMetaRecord>> {
        let q =
            format!("{SESSION_META_SELECT} WHERE status IN ('orphan_create', 'pending_delete')");
        sqlx::query_as::<_, SessionMetaRecord>(&q)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| MacoError::database(e.to_string()))
    }

    pub fn now() -> String {
        Utc::now().to_rfc3339()
    }

    pub fn new_record(
        session_id: String,
        title: Option<String>,
        model_id: Option<String>,
        project_root: Option<String>,
        permission_mode: Option<AgentPermissionMode>,
        git_worktree_enabled: Option<bool>,
        git_branch_prefix: Option<String>,
    ) -> SessionMetaRecord {
        let now = Self::now();
        SessionMetaRecord {
            session_id,
            title,
            model_id,
            project_id: None,
            project_root,
            permission_mode: permission_mode.unwrap_or_default().as_str().to_string(),
            git_worktree_enabled: if git_worktree_enabled.unwrap_or(true) {
                1
            } else {
                0
            },
            git_branch_prefix: git_branch_prefix
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| DEFAULT_GIT_BRANCH_PREFIX.to_string()),
            git_worktree_path: None,
            git_worktree_branch: None,
            status: "active".into(),
            created_at: now.clone(),
            updated_at: now,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_rec() -> SessionMetaRecord {
        SessionMetaRepo::new_record(
            "sid".into(),
            None,
            None,
            Some("/tmp/proj".into()),
            None,
            None,
            None,
        )
    }

    #[test]
    fn agent_workspace_root_prefers_worktree_path() {
        let wt = std::env::temp_dir().join("maco-agent-wt-test");
        std::fs::create_dir_all(&wt).expect("create temp worktree dir");
        let mut rec = base_rec();
        rec.git_worktree_path = Some(wt.to_string_lossy().into_owned());
        assert_eq!(
            SessionMetaRepo::agent_workspace_root_from_record(&rec),
            Some(wt.to_string_lossy().into_owned())
        );
        let _ = std::fs::remove_dir_all(&wt);
    }

    #[test]
    fn agent_workspace_root_disabled_uses_project() {
        let dir = std::env::temp_dir().join("maco-agent-project-test");
        std::fs::create_dir_all(&dir).expect("create project dir");
        let mut rec = base_rec();
        rec.project_root = Some(dir.to_string_lossy().into_owned());
        rec.git_worktree_enabled = 0;
        assert_eq!(
            SessionMetaRepo::agent_workspace_root_from_record(&rec),
            Some(dir.to_string_lossy().into_owned())
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
