use maco_core::{MacoError, MacoResult};
use sqlx::SqlitePool;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct ArtifactRecord {
    pub id: String,
    pub session_id: String,
    pub filename: String,
    pub mime_type: String,
    pub size_bytes: i64,
    pub storage_path: String,
    pub checksum: Option<String>,
    pub created_at: String,
}

#[derive(Clone)]
pub struct ArtifactRepo {
    pool: SqlitePool,
}

impl ArtifactRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn insert(
        &self,
        session_id: &str,
        filename: &str,
        mime_type: &str,
        size_bytes: i64,
        storage_path: &str,
        checksum: Option<&str>,
    ) -> MacoResult<ArtifactRecord> {
        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO maco_artifacts (id, session_id, filename, mime_type, size_bytes, storage_path, checksum, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(session_id)
        .bind(filename)
        .bind(mime_type)
        .bind(size_bytes)
        .bind(storage_path)
        .bind(checksum)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(|e| MacoError::database(e.to_string()))?;
        Ok(ArtifactRecord {
            id,
            session_id: session_id.to_string(),
            filename: filename.to_string(),
            mime_type: mime_type.to_string(),
            size_bytes,
            storage_path: storage_path.to_string(),
            checksum: checksum.map(str::to_string),
            created_at: now,
        })
    }
}
