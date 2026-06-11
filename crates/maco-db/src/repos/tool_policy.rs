use maco_core::{MacoError, MacoResult};
use sqlx::SqlitePool;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct ToolPolicyRecord {
    pub id: String,
    pub tool_pattern: String,
    pub source_type: String,
    pub action: String,
    pub enabled: i64,
    pub created_at: String,
}

#[derive(Clone)]
pub struct ToolPolicyRepo {
    pool: SqlitePool,
}

impl ToolPolicyRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn list_enabled(&self) -> MacoResult<Vec<ToolPolicyRecord>> {
        sqlx::query_as::<_, ToolPolicyRecord>(
            "SELECT id, tool_pattern, source_type, action, enabled, created_at
             FROM maco_tool_policies WHERE enabled = 1 ORDER BY created_at",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| MacoError::database(e.to_string()))
    }

    pub async fn upsert_seed(
        &self,
        tool_pattern: &str,
        source_type: &str,
        action: &str,
    ) -> MacoResult<()> {
        let exists: Option<(String,)> = sqlx::query_as(
            "SELECT id FROM maco_tool_policies WHERE tool_pattern = ? AND source_type = ?",
        )
        .bind(tool_pattern)
        .bind(source_type)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| MacoError::database(e.to_string()))?;
        if exists.is_some() {
            return Ok(());
        }
        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO maco_tool_policies (id, tool_pattern, source_type, action, enabled, created_at)
             VALUES (?, ?, ?, ?, 1, ?)",
        )
        .bind(id)
        .bind(tool_pattern)
        .bind(source_type)
        .bind(action)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|e| MacoError::database(e.to_string()))?;
        Ok(())
    }
}

pub async fn seed_tool_policies(repo: &ToolPolicyRepo) -> MacoResult<()> {
    repo.upsert_seed("delete_*", "mcp", "confirm").await?;
    repo.upsert_seed("write_*", "mcp", "confirm").await?;
    repo.upsert_seed("bash", "tool", "confirm").await?;
    Ok(())
}
