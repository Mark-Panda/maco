//! `maco_run_events`：Run SSE 事件持久化回放。

use maco_core::{MacoError, SseEnvelope};
use sqlx::SqlitePool;

#[derive(Clone)]
pub struct RunEventRepo {
    pool: SqlitePool,
}

impl RunEventRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn append(&self, env: &SseEnvelope) -> maco_core::MacoResult<()> {
        let payload = serde_json::to_string(&env.payload)
            .map_err(|e| MacoError::database(format!("serialize run event payload: {e}")))?;
        let now = chrono::Utc::now().to_rfc3339();
        let seq = i64::try_from(env.seq)
            .map_err(|_| MacoError::database("run event seq exceeds SQLite INTEGER range"))?;
        sqlx::query(
            // Publish retries should be idempotent: the first event for a run/seq wins.
            "INSERT OR IGNORE INTO maco_run_events
                (run_id, seq, event_type, payload, created_at)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&env.run_id)
        .bind(seq)
        .bind(&env.event_type)
        .bind(payload)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|e| MacoError::database(e.to_string()))?;
        Ok(())
    }

    pub async fn list_after(
        &self,
        run_id: &str,
        after_seq: Option<u64>,
        limit: u32,
    ) -> maco_core::MacoResult<Vec<SseEnvelope>> {
        let limit = limit.clamp(1, 1_000) as i64;
        let after_seq = after_seq.unwrap_or(0) as i64;
        let rows = sqlx::query_as::<_, RunEventRow>(
            "SELECT run_id, seq, event_type, payload
             FROM maco_run_events
             WHERE run_id = ? AND seq > ?
             ORDER BY seq ASC
             LIMIT ?",
        )
        .bind(run_id)
        .bind(after_seq)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| MacoError::database(e.to_string()))?;

        rows.into_iter().map(RunEventRow::into_envelope).collect()
    }

    pub async fn purge_older_than_days(&self, days: i64) -> maco_core::MacoResult<u64> {
        let cutoff = (chrono::Utc::now() - chrono::Duration::days(days)).to_rfc3339();
        let result = sqlx::query("DELETE FROM maco_run_events WHERE created_at < ?")
            .bind(cutoff)
            .execute(&self.pool)
            .await
            .map_err(|e| MacoError::database(e.to_string()))?;
        Ok(result.rows_affected())
    }
}

#[derive(sqlx::FromRow)]
struct RunEventRow {
    run_id: String,
    seq: i64,
    event_type: String,
    payload: String,
}

impl RunEventRow {
    fn into_envelope(self) -> maco_core::MacoResult<SseEnvelope> {
        let payload = serde_json::from_str(&self.payload)
            .map_err(|e| MacoError::database(format!("parse run event payload: {e}")))?;
        Ok(SseEnvelope {
            event_type: self.event_type,
            run_id: self.run_id,
            seq: self.seq as u64,
            payload,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn in_memory_repo() -> RunEventRepo {
        let pool = SqlitePool::connect(":memory:").await.expect("sqlite");
        sqlx::query(
            "CREATE TABLE maco_run_events (
                run_id TEXT NOT NULL,
                seq INTEGER NOT NULL,
                event_type TEXT NOT NULL,
                payload TEXT NOT NULL,
                created_at TEXT NOT NULL,
                PRIMARY KEY (run_id, seq)
            )",
        )
        .execute(&pool)
        .await
        .expect("schema");
        RunEventRepo::new(pool)
    }

    #[tokio::test]
    async fn list_after_returns_events_newer_than_seq() {
        let repo = in_memory_repo().await;

        repo.append(&SseEnvelope {
            event_type: "text".into(),
            run_id: "run-1".into(),
            seq: 1,
            payload: serde_json::json!({ "content": "old" }),
        })
        .await
        .expect("append old");
        repo.append(&SseEnvelope {
            event_type: "text".into(),
            run_id: "run-1".into(),
            seq: 2,
            payload: serde_json::json!({ "content": "new" }),
        })
        .await
        .expect("append new");

        let events = repo.list_after("run-1", Some(1), 50).await.expect("list");

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].seq, 2);
        assert_eq!(events[0].payload["content"], "new");
    }

    #[tokio::test]
    async fn purge_older_than_days_removes_stale_events_only() {
        let repo = in_memory_repo().await;
        sqlx::query(
            "INSERT INTO maco_run_events (run_id, seq, event_type, payload, created_at)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind("run-1")
        .bind(1_i64)
        .bind("text")
        .bind(r#"{"content":"stale"}"#)
        .bind((chrono::Utc::now() - chrono::Duration::days(31)).to_rfc3339())
        .execute(&repo.pool)
        .await
        .expect("insert stale");

        repo.append(&SseEnvelope {
            event_type: "text".into(),
            run_id: "run-1".into(),
            seq: 2,
            payload: serde_json::json!({ "content": "fresh" }),
        })
        .await
        .expect("append fresh");

        let purged = repo.purge_older_than_days(30).await.expect("purge");
        let events = repo.list_after("run-1", None, 50).await.expect("list");

        assert_eq!(purged, 1);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].seq, 2);
    }

    #[tokio::test]
    async fn append_ignores_duplicate_run_seq() {
        let repo = in_memory_repo().await;
        repo.append(&SseEnvelope {
            event_type: "text".into(),
            run_id: "run-1".into(),
            seq: 1,
            payload: serde_json::json!({ "content": "first" }),
        })
        .await
        .expect("append first");
        repo.append(&SseEnvelope {
            event_type: "text".into(),
            run_id: "run-1".into(),
            seq: 1,
            payload: serde_json::json!({ "content": "duplicate" }),
        })
        .await
        .expect("append duplicate");

        let events = repo.list_after("run-1", None, 50).await.expect("list");

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].payload["content"], "first");
    }
}
