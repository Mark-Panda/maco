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

    pub async fn get_by_name(&self, name: &str) -> MacoResult<Option<McpServerRecord>> {
        sqlx::query_as::<_, McpServerRecord>("SELECT * FROM maco_mcp_servers WHERE name = ?")
            .bind(name)
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

    #[allow(clippy::too_many_arguments)]
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

/// DB 中内置 filesystem MCP 服务名。
pub const FILESYSTEM_MCP_NAME: &str = "filesystem";
const FILESYSTEM_MCP_PACKAGE: &str = "@modelcontextprotocol/server-filesystem";

/// 构造 filesystem MCP stdio 参数 JSON（`npx -y @modelcontextprotocol/server-filesystem <roots...>`）。
pub fn filesystem_mcp_args(allowed_roots: &[String]) -> MacoResult<String> {
    let mut args = vec!["-y".to_string(), FILESYSTEM_MCP_PACKAGE.to_string()];
    args.extend(allowed_roots.iter().cloned());
    serde_json::to_string(&args).map_err(|e| MacoError::config(format!("mcp args json: {e}")))
}

/// 首次初始化时注册 filesystem MCP，根目录指向 `tmp_dir`。
pub async fn seed_default_filesystem_mcp(
    repo: &McpServerRepo,
    tmp_dir: &std::path::Path,
) -> MacoResult<()> {
    if repo.get_by_name(FILESYSTEM_MCP_NAME).await?.is_some() {
        return Ok(());
    }
    let root = tmp_dir.to_string_lossy().into_owned();
    let args = filesystem_mcp_args(&[root])?;
    repo.insert(FILESYSTEM_MCP_NAME, "stdio", Some("npx"), &args, None, "{}")
        .await?;
    Ok(())
}
