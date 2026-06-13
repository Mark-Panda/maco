//! HTTP API 路由与 handler 实现。

use crate::AppState;
use axum::{Router, http::StatusCode, response::IntoResponse};
use maco_core::MacoError;

/// 挂载于 `/api` 下的全部 REST 与 SSE 端点。
pub fn api_router() -> Router<AppState> {
    Router::new()
        // 健康检查与治理状态
        .merge(crate::system_routes::router())
        // 会话 CRUD、ReAct plan/todo 与 worktree
        .merge(crate::session_routes::router())
        .merge(crate::artifact_routes::router())
        // Run 状态查询、HITL/Elicitation 恢复、子 Agent 审计
        .merge(crate::run_routes::router())
        // 模型配置
        .merge(crate::model_routes::router())
        // 聊天 SSE 与中断
        .merge(crate::chat_routes::router())
        // 全局 Memory
        .merge(crate::memory_routes::router())
        // MCP 服务配置
        .merge(crate::mcp_routes::router())
        // HITL 工具策略
        .merge(crate::tool_policy_routes::router())
        // Skill 管理
        .merge(crate::skill_routes::router())
        // API Token 管理
        .merge(crate::auth_token_routes::router())
        // 用量统计
        .merge(crate::usage_routes::router())
        // 后台定时任务
        .merge(crate::job_routes::router())
}

/// 将 `MacoError` 映射为 HTTP 状态码的包装类型。
pub(crate) struct ApiError(MacoError);

impl From<MacoError> for ApiError {
    fn from(value: MacoError) -> Self {
        Self(value)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let status = match &self.0 {
            MacoError::NotFound(_) => StatusCode::NOT_FOUND,
            MacoError::Conflict(_) => StatusCode::CONFLICT,
            MacoError::Config(_) => StatusCode::BAD_REQUEST,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (status, self.0.to_string()).into_response()
    }
}
