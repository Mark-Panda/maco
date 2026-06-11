use maco_core::{MacoError, MacoResult};
use sqlx::SqlitePool;

#[derive(Clone)]
pub struct CallbackLogRepo {
    pool: SqlitePool,
}

impl CallbackLogRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn insert_started(
        &self,
        session_id: &str,
        run_id: &str,
        span_id: &str,
        callback_type: &str,
        input: Option<&str>,
        tool_name: Option<&str>,
    ) -> MacoResult<()> {
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO maco_callback_logs (session_id, run_id, span_id, callback_type, tool_name, input, status, created_at)
             VALUES (?, ?, ?, ?, ?, ?, 'started', ?)",
        )
        .bind(session_id)
        .bind(run_id)
        .bind(span_id)
        .bind(callback_type)
        .bind(tool_name)
        .bind(input)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|e| MacoError::database(e.to_string()))?;
        Ok(())
    }

    pub async fn complete_span(
        &self,
        span_id: &str,
        output: Option<&str>,
        status: &str,
        duration_ms: i64,
        error_message: Option<&str>,
    ) -> MacoResult<()> {
        sqlx::query(
            "UPDATE maco_callback_logs SET output = ?, status = ?, duration_ms = ?, error_message = ?
             WHERE span_id = ? AND status = 'started'",
        )
        .bind(output)
        .bind(status)
        .bind(duration_ms)
        .bind(error_message)
        .bind(span_id)
        .execute(&self.pool)
        .await
        .map_err(|e| MacoError::database(e.to_string()))?;
        Ok(())
    }

    pub async fn purge_older_than_days(&self, days: i64) -> MacoResult<u64> {
        let cutoff = (chrono::Utc::now() - chrono::Duration::days(days)).to_rfc3339();
        let result = sqlx::query("DELETE FROM maco_callback_logs WHERE created_at < ?")
            .bind(cutoff)
            .execute(&self.pool)
            .await
            .map_err(|e| MacoError::database(e.to_string()))?;
        Ok(result.rows_affected())
    }
}
