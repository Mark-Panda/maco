//! utoipa OpenAPI 文档定义与 Swagger UI 挂载用的 schema。

#![allow(dead_code)]

use utoipa::OpenApi;

/// `GET /health` 响应体。
#[derive(utoipa::ToSchema, serde::Serialize)]
pub struct HealthResponse {
    /// 数据库连通状态。
    pub db: String,
    /// 服务绑定地址。
    pub bind: String,
}

/// 会话元数据（列表/详情）。
#[derive(utoipa::ToSchema, serde::Serialize)]
pub struct SessionMetaDoc {
    /// 会话 ID（与 adk session 一致）。
    pub session_id: String,
    /// 显示标题。
    pub title: Option<String>,
    /// 绑定模型 ID。
    pub model_id: Option<String>,
    /// 生命周期状态（`active` / `deleted` 等）。
    pub status: String,
}

/// `POST /sessions` 请求体。
#[derive(utoipa::ToSchema, serde::Deserialize)]
pub struct CreateSessionDoc {
    /// 会话标题。
    pub title: Option<String>,
    /// 初始模型 ID。
    pub model_id: Option<String>,
}

/// `POST /chat` 请求体。
#[derive(utoipa::ToSchema, serde::Deserialize)]
pub struct ChatRequestDoc {
    /// 目标会话 ID。
    pub session_id: String,
    /// 用户消息文本。
    pub message: String,
    /// 覆盖使用的模型 ID（可选）。
    pub model_id: Option<String>,
}

/// 模型配置视图（api_key 已脱敏）。
#[derive(utoipa::ToSchema, serde::Serialize)]
pub struct ModelViewDoc {
    /// 模型配置 ID。
    pub id: String,
    /// 显示名称。
    pub name: String,
    /// 提供商：`openai` / `anthropic`。
    pub provider: String,
    /// 上游模型标识。
    pub model_id: String,
    /// 自定义 API Base URL。
    pub base_url: Option<String>,
    /// 是否为默认模型。
    pub is_default: bool,
    /// 是否已配置 API Key。
    pub has_api_key: bool,
}

/// `POST/PATCH /models` 请求体。
#[derive(utoipa::ToSchema, serde::Deserialize)]
pub struct ModelUpsertDoc {
    /// 显示名称。
    pub name: String,
    /// 提供商。
    pub provider: String,
    /// 上游模型标识。
    pub model_id: String,
    /// 自定义 Base URL。
    pub base_url: Option<String>,
    /// 内联 API Key（可选）。
    pub api_key: Option<String>,
    /// 环境变量名（API Key 兜底）。
    pub api_key_env: Option<String>,
    /// 是否设为默认模型。
    pub is_default: bool,
}

/// Memory 检索 query 参数。
#[derive(utoipa::ToSchema, serde::Deserialize)]
pub struct MemorySearchQueryDoc {
    /// 搜索关键词。
    pub q: String,
}

/// `GET /guardrail/status` 响应体。
#[derive(utoipa::ToSchema, serde::Serialize)]
pub struct GuardrailStatusDoc {
    /// 是否启用 PII 脱敏。
    pub pii_enabled: bool,
    /// 日志脱敏级别（如 `basic`）。
    pub log_redact: String,
}

#[utoipa::path(
    get,
    path = "/api/health",
    tag = "system",
    responses((status = 200, description = "服务健康检查", body = HealthResponse))
)]
fn health_doc() {}

#[utoipa::path(
    get,
    path = "/api/sessions",
    tag = "sessions",
    responses((status = 200, description = "列出所有会话", body = [SessionMetaDoc]))
)]
fn list_sessions_doc() {}

#[utoipa::path(
    post,
    path = "/api/sessions",
    tag = "sessions",
    request_body = CreateSessionDoc,
    responses((status = 200, description = "创建会话", body = SessionMetaDoc))
)]
fn create_session_doc() {}

#[utoipa::path(
    post,
    path = "/api/chat",
    tag = "chat",
    request_body = ChatRequestDoc,
    responses((status = 200, description = "SSE 流式返回 SseEnvelope 事件"))
)]
fn chat_doc() {}

#[utoipa::path(
    get,
    path = "/api/models",
    tag = "models",
    responses((status = 200, description = "列出模型配置", body = [ModelViewDoc]))
)]
fn list_models_doc() {}

#[utoipa::path(
    post,
    path = "/api/models",
    tag = "models",
    request_body = ModelUpsertDoc,
    responses((status = 200, description = "创建模型", body = ModelViewDoc))
)]
fn create_model_doc() {}

#[utoipa::path(
    get,
    path = "/api/memory/search",
    tag = "memory",
    params(("q" = String, Query, description = "搜索关键词")),
    responses((status = 200, description = "Memory 检索结果"))
)]
fn memory_search_doc() {}

#[utoipa::path(
    get,
    path = "/api/sessions/{id}/export",
    tag = "sessions",
    params(("id" = String, Path, description = "会话 ID")),
    responses((status = 200, description = "导出 Markdown", content_type = "text/markdown"))
)]
fn export_session_doc() {}

#[utoipa::path(
    get,
    path = "/api/guardrail/status",
    tag = "system",
    responses((status = 200, description = "治理/脱敏配置", body = GuardrailStatusDoc))
)]
fn guardrail_status_doc() {}

/// OpenAPI 文档根（挂载于 `/api/docs`）。
#[derive(OpenApi)]
#[openapi(
    info(
        title = "maco API",
        version = "0.1.0",
        description = "个人 Agent 服务 — 会话、聊天（SSE）、模型、Memory、定时任务等。"
    ),
    paths(
        health_doc,
        list_sessions_doc,
        create_session_doc,
        chat_doc,
        list_models_doc,
        create_model_doc,
        memory_search_doc,
        export_session_doc,
        guardrail_status_doc,
    ),
    components(schemas(
        HealthResponse,
        SessionMetaDoc,
        CreateSessionDoc,
        ChatRequestDoc,
        ModelViewDoc,
        ModelUpsertDoc,
        MemorySearchQueryDoc,
        GuardrailStatusDoc,
    )),
    tags(
        (name = "system", description = "健康检查与治理"),
        (name = "sessions", description = "会话 CRUD 与导出"),
        (name = "chat", description = "聊天 SSE"),
        (name = "models", description = "LLM 模型配置"),
        (name = "memory", description = "长期记忆"),
    )
)]
pub struct ApiDoc;
