//! ReAct 计划与待办持久化（`maco_react_plans` / `maco_react_todos`）。

use maco_core::{MacoError, MacoResult};
use sqlx::SqlitePool;
use uuid::Uuid;

/// 会话级任务计划（Markdown 全文 + 乐观锁版本号）。
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct PlanRecord {
    /// 所属会话 ID。
    pub session_id: String,
    /// 计划 Markdown 全文。
    pub content: String,
    /// 乐观锁版本号。
    pub version: i64,
    /// 最后更新时间。
    pub updated_at: String,
}

/// 会话下单条待办（由 `task_key` 唯一标识）。
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct TodoRecord {
    /// 记录 ID。
    pub id: String,
    /// 所属会话 ID。
    pub session_id: String,
    /// 稳定业务键（同会话内唯一）。
    pub task_key: String,
    /// 待办标题。
    pub title: String,
    /// 状态（`pending` / `in_progress` / `done` 等）。
    pub status: String,
    /// 展示排序权重。
    pub sort_order: i64,
    /// 创建时间。
    pub created_at: String,
    /// 最后更新时间。
    pub updated_at: String,
}

/// ReAct 计划/待办的数据访问层。
#[derive(Clone)]
pub struct ReactRepo {
    pool: SqlitePool,
}

impl ReactRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// 读取指定会话的计划；不存在时返回 `None`。
    pub async fn get_plan(&self, session_id: &str) -> MacoResult<Option<PlanRecord>> {
        sqlx::query_as::<_, PlanRecord>(
            "SELECT session_id, content, version, updated_at FROM maco_react_plans WHERE session_id = ?",
        )
        .bind(session_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| MacoError::database(e.to_string()))
    }

    /// 创建或更新计划；可选 `expected_version` 用于乐观锁冲突检测。
    pub async fn upsert_plan(
        &self,
        session_id: &str,
        content: &str,
        expected_version: Option<i64>,
    ) -> MacoResult<PlanRecord> {
        if let Some(existing) = self.get_plan(session_id).await? {
            if let Some(v) = expected_version {
                if existing.version != v {
                    return Err(MacoError::conflict(format!(
                        "plan version mismatch: expected {v}, got {}",
                        existing.version
                    )));
                }
            }
            let new_version = existing.version + 1;
            let now = chrono::Utc::now().to_rfc3339();
            sqlx::query(
                "UPDATE maco_react_plans SET content = ?, version = ?, updated_at = ? WHERE session_id = ?",
            )
            .bind(content)
            .bind(new_version)
            .bind(&now)
            .bind(session_id)
            .execute(&self.pool)
            .await
            .map_err(|e| MacoError::database(e.to_string()))?;
            return Ok(PlanRecord {
                session_id: session_id.to_string(),
                content: content.to_string(),
                version: new_version,
                updated_at: now,
            });
        }
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO maco_react_plans (session_id, content, version, updated_at) VALUES (?, ?, 1, ?)",
        )
        .bind(session_id)
        .bind(content)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(|e| MacoError::database(e.to_string()))?;
        Ok(PlanRecord {
            session_id: session_id.to_string(),
            content: content.to_string(),
            version: 1,
            updated_at: now,
        })
    }

    /// 列出会话下全部待办，按 `sort_order` 排序。
    pub async fn list_todos(&self, session_id: &str) -> MacoResult<Vec<TodoRecord>> {
        sqlx::query_as::<_, TodoRecord>(
            "SELECT id, session_id, task_key, title, status, sort_order, created_at, updated_at
             FROM maco_react_todos WHERE session_id = ? ORDER BY sort_order",
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| MacoError::database(e.to_string()))
    }

    /// 更新待办状态（pending / in_progress / completed 等）。
    pub async fn patch_todo_status(
        &self,
        session_id: &str,
        task_key: &str,
        status: &str,
    ) -> MacoResult<TodoRecord> {
        let now = chrono::Utc::now().to_rfc3339();
        let rows = sqlx::query(
            "UPDATE maco_react_todos SET status = ?, updated_at = ? WHERE session_id = ? AND task_key = ?",
        )
        .bind(status)
        .bind(&now)
        .bind(session_id)
        .bind(task_key)
        .execute(&self.pool)
        .await
        .map_err(|e| MacoError::database(e.to_string()))?;
        if rows.rows_affected() == 0 {
            return Err(MacoError::not_found("todo not found"));
        }
        sqlx::query_as::<_, TodoRecord>(
            "SELECT id, session_id, task_key, title, status, sort_order, created_at, updated_at
             FROM maco_react_todos WHERE session_id = ? AND task_key = ?",
        )
        .bind(session_id)
        .bind(task_key)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| MacoError::database(e.to_string()))
    }

    /// Agent 工具 `upsert_todo` 落库：按 `(session_id, task_key)` 插入或更新标题。
    pub async fn upsert_todo(
        &self,
        session_id: &str,
        task_key: &str,
        title: &str,
        sort_order: i64,
    ) -> MacoResult<TodoRecord> {
        let now = chrono::Utc::now().to_rfc3339();
        if let Ok(existing) = sqlx::query_as::<_, TodoRecord>(
            "SELECT id, session_id, task_key, title, status, sort_order, created_at, updated_at
             FROM maco_react_todos WHERE session_id = ? AND task_key = ?",
        )
        .bind(session_id)
        .bind(task_key)
        .fetch_one(&self.pool)
        .await
        {
            sqlx::query(
                "UPDATE maco_react_todos SET title = ?, sort_order = ?, updated_at = ? WHERE id = ?",
            )
            .bind(title)
            .bind(sort_order)
            .bind(&now)
            .bind(&existing.id)
            .execute(&self.pool)
            .await
            .map_err(|e| MacoError::database(e.to_string()))?;
            return Ok(TodoRecord {
                title: title.to_string(),
                sort_order,
                updated_at: now,
                ..existing
            });
        }
        let id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO maco_react_todos (id, session_id, task_key, title, status, sort_order, created_at, updated_at)
             VALUES (?, ?, ?, ?, 'pending', ?, ?, ?)",
        )
        .bind(&id)
        .bind(session_id)
        .bind(task_key)
        .bind(title)
        .bind(sort_order)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(|e| MacoError::database(e.to_string()))?;
        Ok(TodoRecord {
            id,
            session_id: session_id.to_string(),
            task_key: task_key.to_string(),
            title: title.to_string(),
            status: "pending".into(),
            sort_order,
            created_at: now.clone(),
            updated_at: now,
        })
    }
}
