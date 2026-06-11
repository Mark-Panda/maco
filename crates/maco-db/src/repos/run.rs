use maco_core::{MacoError, MacoResult, RUN_STATUS_RUNNING};
use sqlx::SqlitePool;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct RunRecord {
    pub id: String,
    pub session_id: String,
    pub status: String,
    pub resume_context: Option<String>,
    pub superseded_by: Option<String>,
    pub error_message: Option<String>,
    pub last_seq: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone)]
pub struct RunRepo {
    pool: SqlitePool,
}

impl RunRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

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
}
