#![allow(dead_code)]

use utoipa::OpenApi;

#[derive(utoipa::ToSchema, serde::Serialize)]
pub struct HealthResponse {
    pub db: String,
    pub bind: String,
}

#[derive(utoipa::ToSchema, serde::Serialize)]
pub struct SessionMetaDoc {
    pub session_id: String,
    pub title: Option<String>,
    pub model_id: Option<String>,
    pub status: String,
}

#[derive(utoipa::ToSchema, serde::Deserialize)]
pub struct CreateSessionDoc {
    pub title: Option<String>,
    pub model_id: Option<String>,
}

#[derive(utoipa::ToSchema, serde::Deserialize)]
pub struct ChatRequestDoc {
    pub session_id: String,
    pub message: String,
    pub model_id: Option<String>,
}

#[derive(utoipa::ToSchema, serde::Serialize)]
pub struct ModelViewDoc {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub model_id: String,
    pub base_url: Option<String>,
    pub is_default: bool,
    pub has_api_key: bool,
}

#[derive(utoipa::ToSchema, serde::Deserialize)]
pub struct ModelUpsertDoc {
    pub name: String,
    pub provider: String,
    pub model_id: String,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub api_key_env: Option<String>,
    pub is_default: bool,
}

#[derive(utoipa::ToSchema, serde::Deserialize)]
pub struct MemorySearchQueryDoc {
    pub q: String,
}

#[derive(utoipa::ToSchema, serde::Serialize)]
pub struct GuardrailStatusDoc {
    pub pii_enabled: bool,
    pub log_redact: String,
}

#[utoipa::path(
    get,
    path = "/api/health",
    tag = "system",
    responses((status = 200, description = "Service health", body = HealthResponse))
)]
fn health_doc() {}

#[utoipa::path(
    get,
    path = "/api/sessions",
    tag = "sessions",
    responses((status = 200, description = "List sessions", body = [SessionMetaDoc]))
)]
fn list_sessions_doc() {}

#[utoipa::path(
    post,
    path = "/api/sessions",
    tag = "sessions",
    request_body = CreateSessionDoc,
    responses((status = 200, description = "Created session", body = SessionMetaDoc))
)]
fn create_session_doc() {}

#[utoipa::path(
    post,
    path = "/api/chat",
    tag = "chat",
    request_body = ChatRequestDoc,
    responses((status = 200, description = "SSE stream of SseEnvelope events"))
)]
fn chat_doc() {}

#[utoipa::path(
    get,
    path = "/api/models",
    tag = "models",
    responses((status = 200, description = "List models", body = [ModelViewDoc]))
)]
fn list_models_doc() {}

#[utoipa::path(
    post,
    path = "/api/models",
    tag = "models",
    request_body = ModelUpsertDoc,
    responses((status = 200, description = "Created model", body = ModelViewDoc))
)]
fn create_model_doc() {}

#[utoipa::path(
    get,
    path = "/api/memory/search",
    tag = "memory",
    params(("q" = String, Query, description = "Search query")),
    responses((status = 200, description = "Memory search results"))
)]
fn memory_search_doc() {}

#[utoipa::path(
    get,
    path = "/api/sessions/{id}/export",
    tag = "sessions",
    params(("id" = String, Path, description = "Session ID")),
    responses((status = 200, description = "Markdown export", content_type = "text/markdown"))
)]
fn export_session_doc() {}

#[utoipa::path(
    get,
    path = "/api/guardrail/status",
    tag = "system",
    responses((status = 200, description = "Guardrail config", body = GuardrailStatusDoc))
)]
fn guardrail_status_doc() {}

#[derive(OpenApi)]
#[openapi(
    info(
        title = "maco API",
        version = "0.1.0",
        description = "Personal Agent service — sessions, chat (SSE), models, memory, jobs."
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
        (name = "system", description = "Health & guardrails"),
        (name = "sessions", description = "Session CRUD & export"),
        (name = "chat", description = "Chat SSE"),
        (name = "models", description = "LLM model configuration"),
        (name = "memory", description = "Long-term memory"),
    )
)]
pub struct ApiDoc;
