use chrono::Utc;
use maco_core::{MacoError, MacoResult};
use sqlx::SqlitePool;

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct SessionMetaRecord {
    pub session_id: String,
    pub title: Option<String>,
    pub model_id: Option<String>,
    pub project_id: Option<String>,
    pub status: String,
    pub created_at: String,
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
            "INSERT INTO maco_session_meta (session_id, title, model_id, project_id, status, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&rec.session_id)
        .bind(&rec.title)
        .bind(&rec.model_id)
        .bind(&rec.project_id)
        .bind(&rec.status)
        .bind(&rec.created_at)
        .bind(&rec.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|e| MacoError::database(e.to_string()))?;
        Ok(())
    }

    pub async fn get(&self, session_id: &str) -> MacoResult<Option<SessionMetaRecord>> {
        sqlx::query_as::<_, SessionMetaRecord>(
            "SELECT session_id, title, model_id, project_id, status, created_at, updated_at
             FROM maco_session_meta WHERE session_id = ?",
        )
        .bind(session_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| MacoError::database(e.to_string()))
    }

    pub async fn list_active(&self) -> MacoResult<Vec<SessionMetaRecord>> {
        sqlx::query_as::<_, SessionMetaRecord>(
            "SELECT session_id, title, model_id, project_id, status, created_at, updated_at
             FROM maco_session_meta WHERE status NOT IN ('deleted') ORDER BY updated_at DESC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| MacoError::database(e.to_string()))
    }

    pub async fn list_by_ids(&self, ids: &[String]) -> MacoResult<Vec<SessionMetaRecord>> {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let q = format!(
            "SELECT session_id, title, model_id, project_id, status, created_at, updated_at
             FROM maco_session_meta WHERE session_id IN ({placeholders})"
        );
        let mut query = sqlx::query_as::<_, SessionMetaRecord>(&q);
        for id in ids {
            query = query.bind(id);
        }
        query.fetch_all(&self.pool).await.map_err(|e| MacoError::database(e.to_string()))
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

    pub async fn list_orphans(&self) -> MacoResult<Vec<SessionMetaRecord>> {
        sqlx::query_as::<_, SessionMetaRecord>(
            "SELECT session_id, title, model_id, project_id, status, created_at, updated_at
             FROM maco_session_meta WHERE status IN ('orphan_create', 'pending_delete')",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| MacoError::database(e.to_string()))
    }

    pub fn now() -> String {
        Utc::now().to_rfc3339()
    }

    pub fn new_record(session_id: String, title: Option<String>, model_id: Option<String>) -> SessionMetaRecord {
        let now = Self::now();
        SessionMetaRecord {
            session_id,
            title,
            model_id,
            project_id: None,
            status: "active".into(),
            created_at: now.clone(),
            updated_at: now,
        }
    }
}
