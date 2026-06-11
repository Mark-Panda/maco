//! `maco_mcp_servers` 表：MCP 服务配置（stdio / sse）。

use maco_core::{MacoError, MacoResult};
use sqlx::SqlitePool;
use uuid::Uuid;

/// MCP 服务配置记录。
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize, serde::Deserialize)]
pub struct McpServerRecord {
    /// 配置 ID。
    pub id: String,
    /// 唯一服务名（MCP manager 中的 server id）。
    pub name: String,
    /// 传输类型：`stdio` 或 `sse`。
    pub transport: String,
    /// stdio 可执行命令。
    pub command: Option<String>,
    /// 命令参数 JSON 数组。
    pub args: String,
    /// SSE 端点 URL。
    pub url: Option<String>,
    /// 环境变量 JSON 对象。
    pub env: String,
    /// 是否启用（SQLite 0/1）。
    pub enabled: i64,
    /// 创建时间。
    pub created_at: String,
    /// 最后更新时间。
    pub updated_at: String,
}

#[derive(Clone)]
pub struct McpServerRepo {
    pool: SqlitePool,
}

impl McpServerRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn list(&self) -> MacoResult<Vec<McpServerRecord>> {
        sqlx::query_as::<_, McpServerRecord>("SELECT * FROM maco_mcp_servers ORDER BY name")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| MacoError::database(e.to_string()))
    }

    pub async fn list_enabled(&self) -> MacoResult<Vec<McpServerRecord>> {
        sqlx::query_as::<_, McpServerRecord>(
            "SELECT * FROM maco_mcp_servers WHERE enabled = 1 ORDER BY name",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| MacoError::database(e.to_string()))
    }

    pub async fn get(&self, id: &str) -> MacoResult<Option<McpServerRecord>> {
        sqlx::query_as::<_, McpServerRecord>("SELECT * FROM maco_mcp_servers WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| MacoError::database(e.to_string()))
    }

    pub async fn insert(
        &self,
        name: &str,
        transport: &str,
        command: Option<&str>,
        args: &str,
        url: Option<&str>,
        env: &str,
    ) -> MacoResult<McpServerRecord> {
        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO maco_mcp_servers (id, name, transport, command, args, url, env, enabled, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, 1, ?, ?)",
        )
        .bind(&id)
        .bind(name)
        .bind(transport)
        .bind(command)
        .bind(args)
        .bind(url)
        .bind(env)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(|e| MacoError::database(e.to_string()))?;
        self.get(&id)
            .await?
            .ok_or_else(|| MacoError::database("mcp server missing after insert"))
    }

    pub async fn update(
        &self,
        id: &str,
        name: &str,
        transport: &str,
        command: Option<&str>,
        args: &str,
        url: Option<&str>,
        env: &str,
        enabled: bool,
    ) -> MacoResult<()> {
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "UPDATE maco_mcp_servers SET name = ?, transport = ?, command = ?, args = ?, url = ?, env = ?, enabled = ?, updated_at = ?
             WHERE id = ?",
        )
        .bind(name)
        .bind(transport)
        .bind(command)
        .bind(args)
        .bind(url)
        .bind(env)
        .bind(if enabled { 1 } else { 0 })
        .bind(now)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| MacoError::database(e.to_string()))?;
        Ok(())
    }

    pub async fn delete(&self, id: &str) -> MacoResult<bool> {
        let r = sqlx::query("DELETE FROM maco_mcp_servers WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| MacoError::database(e.to_string()))?;
        Ok(r.rows_affected() > 0)
    }
}

