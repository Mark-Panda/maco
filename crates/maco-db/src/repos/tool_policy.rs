//! `maco_tool_policies` 表：工具级 HITL 策略（allow / confirm / deny）。

use maco_core::{MacoError, MacoResult};
use sqlx::SqlitePool;
use uuid::Uuid;

/// 工具 HITL 策略规则。
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct ToolPolicyRecord {
    /// 规则 ID。
    pub id: String,
    /// 工具名匹配模式（支持通配）。
    pub tool_pattern: String,
    /// 来源类型（`builtin` / `mcp` 等）。
    pub source_type: String,
    /// 动作：`allow` / `confirm` / `deny`。
    pub action: String,
    /// 是否启用（SQLite 0/1）。
    pub enabled: i64,
    /// 创建时间。
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
