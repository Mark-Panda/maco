//! `maco_runs` 表：Run 状态机、resume 上下文与 SSE 序号。

use maco_core::{
    MacoError, MacoResult, RUN_STATUS_AWAITING_USER, RUN_STATUS_FAILED, RUN_STATUS_RUNNING,
};
use sqlx::SqlitePool;
use uuid::Uuid;

/// 单次 Agent 执行记录（与 session 一对多）。
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct RunRecord {
    /// Run 唯一 ID。
    pub id: String,
    /// 所属会话 ID。
    pub session_id: String,
    /// 状态（`pending` / `running` / `awaiting_user` 等）。
    pub status: String,
    /// 暂停恢复上下文 JSON（HITL/Elicitation）。
    pub resume_context: Option<String>,
    /// 被取代的新 Run ID（resume 链路）。
    pub superseded_by: Option<String>,
    /// 失败错误信息。
    pub error_message: Option<String>,
    /// 最后 SSE 事件序号。
    pub last_seq: i64,
    /// 创建时间。
    pub created_at: String,
    /// 最后更新时间。
    pub updated_at: String,
}

/// Run 持久化访问层。
#[derive(Clone)]
pub struct RunRepo {
    pool: SqlitePool,
}

impl RunRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// 创建新 Run 并初始化 `last_seq = 0`。
    pub async fn create(&self, session_id: &str, status: &str) -> MacoResult<RunRecord> {
        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO maco_runs (id, session_id, status, last_seq, created_at, updated_at)
             VALUES (?, ?, ?, 0, ?, ?)",
        )
        .bind(&id)
        .bind(session_id)
        .bind(status)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(|e| MacoError::database(e.to_string()))?;
        Ok(RunRecord {
            id,
            session_id: session_id.to_string(),
            status: status.to_string(),
            resume_context: None,
            superseded_by: None,
            error_message: None,
            last_seq: 0,
            created_at: now.clone(),
            updated_at: now,
        })
    }

    /// 按 ID 查询 Run。
    pub async fn get(&self, run_id: &str) -> MacoResult<Option<RunRecord>> {
        sqlx::query_as::<_, RunRecord>(
            "SELECT id, session_id, status, resume_context, superseded_by, error_message, last_seq, created_at, updated_at
             FROM maco_runs WHERE id = ?",
        )
        .bind(run_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| MacoError::database(e.to_string()))
    }

    /// 更新 Run 状态与可选错误信息。
    pub async fn update_status(
        &self,
        run_id: &str,
        status: &str,
        error_message: Option<&str>,
    ) -> MacoResult<()> {
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "UPDATE maco_runs SET status = ?, error_message = ?, updated_at = ? WHERE id = ?",
        )
        .bind(status)
        .bind(error_message)
        .bind(now)
        .bind(run_id)
        .execute(&self.pool)
        .await
        .map_err(|e| MacoError::database(e.to_string()))?;
        Ok(())
    }

    /// 进入 `awaiting_user` 并写入 resume JSON。
    pub async fn set_awaiting_user(&self, run_id: &str, resume_context: &str) -> MacoResult<()> {
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "UPDATE maco_runs SET status = 'awaiting_user', resume_context = ?, updated_at = ? WHERE id = ?",
        )
        .bind(resume_context)
        .bind(now)
        .bind(run_id)
        .execute(&self.pool)
        .await
        .map_err(|e| MacoError::database(e.to_string()))?;
        Ok(())
    }

    /// 标记旧 Run 已被新 Run 取代（resume 链路）。
    pub async fn set_superseded_by(&self, run_id: &str, new_run_id: &str) -> MacoResult<()> {
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "UPDATE maco_runs SET superseded_by = ?, updated_at = ? WHERE id = ?",
        )
        .bind(new_run_id)
        .bind(now)
        .bind(run_id)
        .execute(&self.pool)
        .await
        .map_err(|e| MacoError::database(e.to_string()))?;
        Ok(())
    }

    /// 递增 SSE 事件序号并返回新值。
    pub async fn bump_seq(&self, run_id: &str) -> MacoResult<u64> {
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "UPDATE maco_runs SET last_seq = last_seq + 1, updated_at = ? WHERE id = ?",
        )
        .bind(now)
        .bind(run_id)
        .execute(&self.pool)
        .await
        .map_err(|e| MacoError::database(e.to_string()))?;
        let row: (i64,) = sqlx::query_as("SELECT last_seq FROM maco_runs WHERE id = ?")
            .bind(run_id)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| MacoError::database(e.to_string()))?;
        Ok(row.0 as u64)
    }

    /// 恢复执行后清空 resume 上下文。
    pub async fn clear_resume_context(&self, run_id: &str) -> MacoResult<()> {
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "UPDATE maco_runs SET resume_context = NULL, updated_at = ? WHERE id = ?",
        )
        .bind(now)
        .bind(run_id)
        .execute(&self.pool)
        .await
        .map_err(|e| MacoError::database(e.to_string()))?;
        Ok(())
    }

    /// 会话是否已有 `running` 状态的 Run。
    pub async fn has_running(&self, session_id: &str) -> MacoResult<bool> {
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM maco_runs WHERE session_id = ? AND status = ?",
        )
        .bind(session_id)
        .bind(RUN_STATUS_RUNNING)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| MacoError::database(e.to_string()))?;
        Ok(row.0 > 0)
    }

    /// 会话当前活跃 Run（`running` / `awaiting_user`，按更新时间最近一条）。
    pub async fn find_active_for_session(
        &self,
        session_id: &str,
    ) -> MacoResult<Option<RunRecord>> {
        sqlx::query_as::<_, RunRecord>(
            "SELECT id, session_id, status, resume_context, superseded_by, error_message, last_seq, created_at, updated_at
             FROM maco_runs
             WHERE session_id = ? AND status IN (?, ?)
             ORDER BY updated_at DESC
             LIMIT 1",
        )
        .bind(session_id)
        .bind(RUN_STATUS_RUNNING)
        .bind(RUN_STATUS_AWAITING_USER)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| MacoError::database(e.to_string()))
    }

    /// 服务重启后将孤儿 `running` / `awaiting_user` 标为失败。
    pub async fn fail_stale_active_runs(&self, reason: &str) -> MacoResult<u64> {
        let now = chrono::Utc::now().to_rfc3339();
        let result = sqlx::query(
            "UPDATE maco_runs SET status = ?, error_message = ?, updated_at = ?
             WHERE status IN (?, ?)",
        )
        .bind(RUN_STATUS_FAILED)
        .bind(reason)
        .bind(&now)
        .bind(RUN_STATUS_RUNNING)
        .bind(RUN_STATUS_AWAITING_USER)
        .execute(&self.pool)
        .await
        .map_err(|e| MacoError::database(e.to_string()))?;
        Ok(result.rows_affected())
    }

    /// 会话是否已有活跃 Run（`running` 或 `awaiting_user`，防叠加新 Run）。
    pub async fn has_active_run(&self, session_id: &str) -> MacoResult<bool> {
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM maco_runs WHERE session_id = ? AND status IN (?, ?)",
        )
        .bind(session_id)
        .bind(RUN_STATUS_RUNNING)
        .bind(RUN_STATUS_AWAITING_USER)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| MacoError::database(e.to_string()))?;
        Ok(row.0 > 0)
    }
}
