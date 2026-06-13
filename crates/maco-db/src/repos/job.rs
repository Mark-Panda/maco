//! `maco_jobs` 表：可调度后台任务（ping / log 等）。

use maco_core::{MacoError, MacoResult};
use sqlx::SqlitePool;
use uuid::Uuid;

/// 定时或手动触发的后台任务记录。
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct JobRecord {
    /// 任务 ID。
    pub id: String,
    /// 显示名称。
    pub name: String,
    /// 任务类型（`ping` / `log` 等）。
    pub job_type: String,
    /// 调度周期（`hourly` / `daily`，可选）。
    pub schedule: Option<String>,
    /// JSON 载荷字符串。
    pub payload: String,
    /// 最近执行状态。
    pub status: String,
    /// 上次执行时间。
    pub last_run_at: Option<String>,
    /// 下次计划执行时间。
    pub next_run_at: Option<String>,
    /// 上次执行结果文本。
    pub result: Option<String>,
    /// 上次执行错误信息。
    pub error_message: Option<String>,
    /// 是否启用（SQLite 0/1）。
    pub enabled: i64,
    /// 创建时间。
    pub created_at: String,
    /// 最后更新时间。
    pub updated_at: String,
}

/// Job 持久化与到期扫描。
#[derive(Clone)]
pub struct JobRepo {
    pool: SqlitePool,
}

impl JobRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn list(&self) -> MacoResult<Vec<JobRecord>> {
        sqlx::query_as::<_, JobRecord>("SELECT * FROM maco_jobs ORDER BY created_at DESC")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| MacoError::database(e.to_string()))
    }

    pub async fn get(&self, id: &str) -> MacoResult<Option<JobRecord>> {
        sqlx::query_as::<_, JobRecord>("SELECT * FROM maco_jobs WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| MacoError::database(e.to_string()))
    }

    pub async fn insert(
        &self,
        name: &str,
        job_type: &str,
        schedule: Option<&str>,
        payload: &str,
        next_run_at: Option<&str>,
    ) -> MacoResult<JobRecord> {
        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO maco_jobs (id, name, job_type, schedule, payload, status, next_run_at, enabled, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, 'pending', ?, 1, ?, ?)",
        )
        .bind(&id)
        .bind(name)
        .bind(job_type)
        .bind(schedule)
        .bind(payload)
        .bind(next_run_at)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(|e| MacoError::database(e.to_string()))?;
        self.get(&id)
            .await?
            .ok_or_else(|| MacoError::database("job missing after insert"))
    }

    pub async fn update_run_result(
        &self,
        id: &str,
        status: &str,
        result: Option<&str>,
        error_message: Option<&str>,
        next_run_at: Option<&str>,
    ) -> MacoResult<()> {
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "UPDATE maco_jobs SET status = ?, result = ?, error_message = ?, last_run_at = ?, next_run_at = ?, updated_at = ? WHERE id = ?",
        )
        .bind(status)
        .bind(result)
        .bind(error_message)
        .bind(&now)
        .bind(next_run_at)
        .bind(&now)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| MacoError::database(e.to_string()))?;
        Ok(())
    }

    pub async fn set_enabled(&self, id: &str, enabled: bool) -> MacoResult<()> {
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query("UPDATE maco_jobs SET enabled = ?, updated_at = ? WHERE id = ?")
            .bind(if enabled { 1 } else { 0 })
            .bind(now)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| MacoError::database(e.to_string()))?;
        Ok(())
    }

    pub async fn delete(&self, id: &str) -> MacoResult<bool> {
        let r = sqlx::query("DELETE FROM maco_jobs WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| MacoError::database(e.to_string()))?;
        Ok(r.rows_affected() > 0)
    }

    pub async fn due_jobs(&self, now_iso: &str) -> MacoResult<Vec<JobRecord>> {
        sqlx::query_as::<_, JobRecord>(
            "SELECT * FROM maco_jobs
             WHERE enabled = 1 AND status IN ('pending', 'completed')
               AND next_run_at IS NOT NULL AND next_run_at <= ?
             ORDER BY next_run_at ASC LIMIT 10",
        )
        .bind(now_iso)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| MacoError::database(e.to_string()))
    }
}
