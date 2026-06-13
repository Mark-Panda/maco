//! 系统健康检查、治理状态与本机能力路由。

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use maco_core::MacoError;
use maco_governance::pii_guardrail_enabled;

use crate::AppState;
use crate::routes::ApiError;

/// 系统路由，挂载于 `/api` 下。
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/health", get(health))
        .route("/guardrail/status", get(guardrail_status))
        .route("/system/pick-directory", post(pick_directory))
}

/// `POST /system/pick-directory` — 弹出本机原生文件夹选择对话框，返回绝对路径。
async fn pick_directory() -> Result<Json<serde_json::Value>, ApiError> {
    let picked = tokio::task::spawn_blocking(crate::directory_picker::pick_directory_blocking)
        .await
        .map_err(|e| MacoError::config(format!("pick directory task: {e}")))?;

    match picked {
        Some(path) => Ok(Json(serde_json::json!({
            "cancelled": false,
            "path": path.to_string_lossy(),
        }))),
        None => Ok(Json(serde_json::json!({ "cancelled": true }))),
    }
}

/// `GET /guardrail/status` — 返回 PII 脱敏、日志脱敏与 worktree 路径守卫配置。
async fn guardrail_status(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "pii_enabled": pii_guardrail_enabled(),
        "log_redact": std::env::var("MACO_LOG_REDACT").unwrap_or_else(|_| "basic".into()),
        "worktree_path_guard": state.agent.harness.worktree_path_guard_enabled().await,
    }))
}

/// `GET /health` — 探活：数据库、MCP 池、Memory、Skill 数量与绑定地址。
async fn health(State(state): State<AppState>) -> impl IntoResponse {
    let mcp = state.agent.mcp_pool.status_summary().await;
    let mcp_status = state.agent.mcp_pool.health_status().await;
    let db = probe_db_health(&state).await;
    let session = probe_session_health(&state).await;
    let memory = probe_memory_health(&state).await;
    let overall = health_overall(
        db.get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("failed"),
        session
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("failed"),
        memory
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("failed"),
        &mcp_status.overall,
    );
    let body = serde_json::json!({
        "overall": overall,
        "db": db,
        "session": session,
        "mcp": mcp,
        "mcp_status": mcp_status,
        "memory": memory,
        "skills": state.agent.adk_skills.enabled_count(),
        "skills_total": state.agent.adk_skills.total_count(),
        "bind": state.runtime.bind_addr,
        "tmp_dir": state.runtime.tmp_dir.to_string_lossy(),
    });
    (health_status_code(overall), Json(body))
}

async fn probe_db_health(state: &AppState) -> serde_json::Value {
    match state.repos.settings.ping().await {
        Ok(_) => serde_json::json!({ "status": "ok" }),
        Err(e) => serde_json::json!({ "status": "failed", "error": e.to_string() }),
    }
}

async fn probe_session_health(state: &AppState) -> serde_json::Value {
    match sqlx::query("SELECT 1")
        .execute(state.agent.adk.session_pool())
        .await
    {
        Ok(_) => serde_json::json!({ "status": "ok" }),
        Err(e) => serde_json::json!({ "status": "failed", "error": e.to_string() }),
    }
}

async fn probe_memory_health(state: &AppState) -> serde_json::Value {
    match sqlx::query("SELECT 1")
        .execute(state.agent.adk.memory_pool())
        .await
    {
        Ok(_) => serde_json::json!({ "status": "ok" }),
        Err(e) => serde_json::json!({ "status": "failed", "error": e.to_string() }),
    }
}

fn health_overall(db: &str, session: &str, memory: &str, mcp: &str) -> &'static str {
    if db == "ok" && session == "ok" && memory == "ok" && mcp == "ok" {
        "ok"
    } else {
        "degraded"
    }
}

fn health_status_code(overall: &str) -> StatusCode {
    if overall == "ok" {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_overall_degrades_when_any_dependency_fails() {
        assert_eq!(health_overall("ok", "ok", "ok", "ok"), "ok");
        assert_eq!(health_overall("failed", "ok", "ok", "ok"), "degraded");
        assert_eq!(health_overall("ok", "failed", "ok", "ok"), "degraded");
        assert_eq!(health_overall("ok", "ok", "failed", "ok"), "degraded");
        assert_eq!(health_overall("ok", "ok", "ok", "degraded"), "degraded");
    }

    #[test]
    fn health_status_code_is_unavailable_when_degraded() {
        assert_eq!(health_status_code("ok"), StatusCode::OK);
        assert_eq!(
            health_status_code("degraded"),
            StatusCode::SERVICE_UNAVAILABLE
        );
    }
}
