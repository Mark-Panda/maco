//! 直接查询 adk `memory.db` 的管理接口（列表等 adk API 未暴露的能力）。

use std::path::Path;

use maco_core::{APP_NAME, MacoError, MacoResult, USER_ID, adk_memory_url};
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::{Row, SqlitePool};
use std::str::FromStr;

/// adk memory.db 原始行（管理 API 用）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct MemoryRow {
    /// 行 ID。
    pub id: i64,
    /// 记忆文本。
    pub content: String,
    /// 作者标识。
    pub author: String,
    /// 写入时间。
    pub timestamp: String,
    /// 来源会话 ID。
    pub session_id: String,
}

pub async fn list_memory_rows(memory_db: &Path, limit: usize) -> MacoResult<Vec<MemoryRow>> {
    let url = adk_memory_url(memory_db);
    let options = SqliteConnectOptions::from_str(&url)
        .map_err(|e| MacoError::database(e.to_string()))?
        .create_if_missing(false);
    let pool = SqlitePool::connect_with(options)
        .await
        .map_err(|e| MacoError::database(e.to_string()))?;
    list_from_pool(&pool, limit).await
}

pub async fn list_from_pool(pool: &SqlitePool, limit: usize) -> MacoResult<Vec<MemoryRow>> {
    let limit = limit.clamp(1, 500) as i64;
    let rows = sqlx::query(
        "SELECT id, content_text, author, timestamp, session_id
         FROM memory_entries
         WHERE app_name = ? AND user_id = ?
         ORDER BY timestamp DESC
         LIMIT ?",
    )
    .bind(APP_NAME)
    .bind(USER_ID)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(|e| MacoError::database(e.to_string()))?;

    Ok(rows
        .into_iter()
        .map(|r| MemoryRow {
            id: r.get("id"),
            content: r.get("content_text"),
            author: r.get("author"),
            timestamp: r.get("timestamp"),
            session_id: r.get("session_id"),
        })
        .collect())
}
