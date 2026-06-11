//! `maco_elicitations` 表：MCP 人机确认请求与响应。

use maco_core::{MacoError, MacoResult};
use sqlx::SqlitePool;
use uuid::Uuid;

/// MCP Elicitation 持久化记录。
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct ElicitationRecord {
    /// 记录 ID。
    pub id: String,
    /// 所属会话 ID。
    pub session_id: String,
    /// 触发时的 Run ID。
    pub run_id: String,
    /// MCP 服务名称。
    pub mcp_server: String,
    /// 请求类型：`form` / `url`。
    pub request_type: String,
    /// 请求载荷 JSON 字符串。
    pub payload: String,
    /// 用户响应 JSON 字符串。
    pub response: Option<String>,
    /// 状态：`pending` / `submitted` / `cancelled` / `expired`。
    pub status: String,
    /// 过期时间。
    pub expires_at: String,
    /// 创建时间。
    pub created_at: String,
    /// 用户响应时间。
    pub responded_at: Option<String>,
}

#[derive(Clone)]
pub struct ElicitationRepo {
    pool: SqlitePool,
}

impl ElicitationRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn insert(
        &self,
        session_id: &str,
        run_id: &str,
        mcp_server: &str,
        request_type: &str,
        payload: &str,
        expires_at: &str,
        id: Option<&str>,
    ) -> MacoResult<ElicitationRecord> {
        let id = id
            .map(str::to_string)
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO maco_elicitation_requests
             (id, session_id, run_id, mcp_server, request_type, payload, status, expires_at, created_at)
             VALUES (?, ?, ?, ?, ?, ?, 'pending', ?, ?)",
        )
        .bind(&id)
        .bind(session_id)
        .bind(run_id)
        .bind(mcp_server)
        .bind(request_type)
        .bind(payload)
        .bind(expires_at)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(|e| MacoError::database(e.to_string()))?;
        Ok(ElicitationRecord {
            id,
            session_id: session_id.to_string(),
            run_id: run_id.to_string(),
            mcp_server: mcp_server.to_string(),
            request_type: request_type.to_string(),
            payload: payload.to_string(),
            response: None,
            status: "pending".into(),
            expires_at: expires_at.to_string(),
            created_at: now,
            responded_at: None,
        })
    }

    pub async fn get(&self, id: &str) -> MacoResult<Option<ElicitationRecord>> {
        sqlx::query_as::<_, ElicitationRecord>(
            "SELECT id, session_id, run_id, mcp_server, request_type, payload, response, status,
                    expires_at, created_at, responded_at
             FROM maco_elicitation_requests WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| MacoError::database(e.to_string()))
    }

    pub async fn list_pending_for_session(
        &self,
        session_id: &str,
    ) -> MacoResult<Vec<ElicitationRecord>> {
        sqlx::query_as::<_, ElicitationRecord>(
            "SELECT id, session_id, run_id, mcp_server, request_type, payload, response, status,
                    expires_at, created_at, responded_at
             FROM maco_elicitation_requests
             WHERE session_id = ? AND status = 'pending'
             ORDER BY created_at ASC",
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| MacoError::database(e.to_string()))
    }

    pub async fn submit_response(
        &self,
        id: &str,
        response: &str,
        status: &str,
    ) -> MacoResult<()> {
        let now = chrono::Utc::now().to_rfc3339();
        let rows = sqlx::query(
            "UPDATE maco_elicitation_requests
             SET response = ?, status = ?, responded_at = ?
             WHERE id = ? AND status = 'pending'",
        )
        .bind(response)
        .bind(status)
        .bind(&now)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| MacoError::database(e.to_string()))?
        .rows_affected();
        if rows == 0 {
            return Err(MacoError::conflict("elicitation not pending"));
        }
        Ok(())
    }

    pub async fn mark_expired(&self, id: &str) -> MacoResult<()> {
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "UPDATE maco_elicitation_requests SET status = 'expired', responded_at = ? WHERE id = ? AND status = 'pending'",
        )
        .bind(&now)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| MacoError::database(e.to_string()))?;
        Ok(())
    }
}

pub fn payload_summary(record: &ElicitationRecord) -> maco_core::PendingElicitation {
    let payload: serde_json::Value =
        serde_json::from_str(&record.payload).unwrap_or(serde_json::json!({}));
    maco_core::PendingElicitation {
        id: record.id.clone(),
        request_type: record.request_type.clone(),
        message: payload
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        schema: payload.get("schema").cloned(),
        url: payload
            .get("url")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        mcp_server: record.mcp_server.clone(),
    }
}
