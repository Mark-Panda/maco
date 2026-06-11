use maco_core::{MacoError, MacoResult};
use sqlx::SqlitePool;

#[derive(Clone)]
pub struct SettingsRepo {
    pool: SqlitePool,
}

impl SettingsRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn get(&self, key: &str) -> MacoResult<Option<String>> {
        let row: Option<(String,)> =
            sqlx::query_as("SELECT value FROM maco_app_settings WHERE key = ?")
                .bind(key)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| MacoError::database(e.to_string()))?;
        Ok(row.map(|r| r.0))
    }

    pub async fn set(&self, key: &str, value: &str) -> MacoResult<()> {
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO maco_app_settings (key, value, updated_at) VALUES (?, ?, ?)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        )
        .bind(key)
        .bind(value)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|e| MacoError::database(e.to_string()))?;
        Ok(())
    }
}

pub async fn seed_defaults(repo: &SettingsRepo) -> MacoResult<()> {
    let memory = r#"{
  "embedding_provider": "openai",
  "embedding_model": "text-embedding-3-small",
  "embedding_api_key_env": "OPENAI_API_KEY",
  "search_top_k": 5,
  "min_score": 0.7
}"#;
    if repo.get("memory").await?.is_none() {
        repo.set("memory", memory).await?;
    }
    Ok(())
}
