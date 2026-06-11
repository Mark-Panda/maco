//! HTTP API 路由与 handler 实现。

use axum::{
    extract::{Extension, Multipart, Path, State},
    http::{header, StatusCode},
    response::IntoResponse,
    routing::{delete, get, patch, post},
    Json, Router,
};
use maco_core::{pending_tools_from_resume, MacoError, RunStatusResponse};
use maco_db::{payload_summary, JobRecord, McpServerRecord};
use maco_harness::elicitation::{action_from_str, ElicitationRespondBody};
use maco_governance::{
    generate_token, hash_token, pii_guardrail_enabled, scopes_json, SCOPE_ADMIN,
};
use maco_harness::SkillLoader;
use serde::{Deserialize, Serialize};
use tokio_stream::{wrappers::BroadcastStream, wrappers::ReceiverStream, StreamExt as _};

use crate::auth::{require_admin, AuthContext};
use crate::export::session_markdown;
use crate::models_api::{list_views, upsert_from_body, ModelUpsertBody, ModelView};
use crate::AppState;

/// 挂载于 `/api` 下的全部 REST 与 SSE 端点。
pub fn api_router() -> Router<AppState> {
    Router::new()
        // 健康检查与治理状态
        .route("/health", get(health))
        .route("/guardrail/status", get(guardrail_status))
        // 会话 CRUD 与 ReAct plan/todo
        .route("/sessions", get(list_sessions).post(create_session))
        .route("/sessions/{id}", patch(update_session).delete(delete_session))
        .route("/sessions/{id}/messages", get(get_session_messages))
        .route("/sessions/{id}/plan", get(get_plan).put(put_plan))
        .route("/sessions/{id}/todos", get(list_todos))
        .route("/sessions/{id}/todos/{task_key}", patch(patch_todo))
        .route(
            "/sessions/{id}/artifacts",
            get(list_artifacts).post(upload_artifact),
        )
        .route(
            "/sessions/{id}/artifacts/{artifact_id}",
            get(download_artifact),
        )
        .route(
            "/sessions/{id}/artifacts/{artifact_id}/preview",
            get(preview_artifact),
        )
        .route("/sessions/{id}/export", get(export_session))
        // Run 状态查询、HITL/Elicitation 恢复
        .route("/sessions/{id}/runs/active", get(get_active_run))
        .route("/sessions/{id}/runs/{run_id}", get(get_run))
        .route("/sessions/{id}/runs/{run_id}/stream", get(stream_run))
        .route("/sessions/{id}/runs/{run_id}/resume", post(resume_run))
        .route("/sessions/{id}/elicitation/pending", get(list_pending_elicitation))
        .route("/elicitation/{id}/respond", post(respond_elicitation))
        // 模型配置
        .route("/models", get(list_models).post(create_model))
        .route("/models/{id}", patch(update_model).delete(delete_model))
        // 聊天 SSE 与中断
        .route("/chat", post(chat_sse))
        .route("/chat/{session_id}/interrupt", post(interrupt_chat))
        // 全局 Memory
        .route("/memory", get(list_memory).post(add_memory).delete(delete_memory))
        .route("/memory/search", get(memory_search))
        // MCP 服务配置
        .route("/mcp/servers", get(list_mcp_servers).post(create_mcp_server))
        .route("/mcp/servers/{id}", patch(update_mcp_server).delete(delete_mcp_server))
        .route("/mcp/reload", post(reload_mcp_pool))
        // HITL 工具策略
        .route(
            "/tool-policies",
            get(list_tool_policies).post(create_tool_policy),
        )
        .route("/tool-policies/reload", post(reload_tool_policies))
        .route(
            "/tool-policies/{id}",
            patch(update_tool_policy).delete(delete_tool_policy),
        )
        // Skill 扫描
        .route("/skills", get(list_skills))
        .route("/skills/{name}", get(get_skill))
        // API Token 管理
        .route("/auth/tokens", get(list_tokens).post(create_token))
        .route("/auth/tokens/{id}", delete(revoke_token))
        // 用量统计
        .route("/usage/summary", get(usage_summary))
        // 后台定时任务
        .route("/jobs", get(list_jobs).post(create_job))
        .route("/jobs/{id}", patch(update_job).delete(delete_job))
        .route("/jobs/{id}/run", post(run_job_now))
        // 本机系统能力（原生文件夹选择等）
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

/// `GET /guardrail/status` — 返回 PII 脱敏与日志脱敏配置状态。
async fn guardrail_status() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "pii_enabled": pii_guardrail_enabled(),
        "log_redact": std::env::var("MACO_LOG_REDACT").unwrap_or_else(|_| "basic".into()),
    }))
}

/// `GET /health` — 探活：数据库、MCP 池、Memory、Skill 数量与绑定地址。
async fn health(State(state): State<AppState>) -> impl IntoResponse {
    let mcp = state.mcp_pool.status_summary().await;
    Json(serde_json::json!({
        "db": "ok",
        "mcp": mcp,
        "memory": "ok",
        "skills": SkillLoader::scan(None).len(),
        "bind": state.bind_addr,
    }))
}

/// `GET /sessions` — 列出所有会话元数据（与 adk session 对齐）。
async fn list_sessions(State(state): State<AppState>) -> Result<Json<Vec<maco_db::SessionMetaRecord>>, ApiError> {
    Ok(Json(state.facade.list_sessions().await?))
}

/// `POST /sessions` 请求体。
#[derive(Deserialize)]
struct CreateSessionBody {
    /// 会话显示标题。
    title: Option<String>,
    /// 初始绑定模型 ID（写入 adk state `user:model`）。
    model_id: Option<String>,
    /// 绑定的本地项目根目录（绝对路径，可含 `~`）。
    project_root: Option<String>,
}

/// `POST /sessions` — 创建新会话。
async fn create_session(
    State(state): State<AppState>,
    Json(body): Json<CreateSessionBody>,
) -> Result<Json<maco_db::SessionMetaRecord>, ApiError> {
    let rec = state
        .facade
        .create_session(body.title, body.model_id, body.project_root)
        .await?;
    if rec.project_root.is_some() {
        if let Err(e) = state.sync_filesystem_mcp().await {
            tracing::warn!("sync filesystem mcp after create session: {e}");
        }
    }
    Ok(Json(rec))
}

/// `PATCH /sessions/{id}` 请求体。
#[derive(Deserialize)]
struct UpdateSessionBody {
    /// 新标题（可选）。
    title: Option<String>,
    /// 新模型 ID（可选，会同步 adk state）。
    model_id: Option<String>,
    /// 项目根目录；传空字符串表示清除绑定。
    project_root: Option<String>,
}

/// `GET /sessions/{id}/messages` — 加载会话历史消息（供前端恢复对话）。
async fn get_session_messages(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<maco_core::SessionMessagesResponse>, ApiError> {
    Ok(Json(state.facade.session_messages(&id).await?))
}

/// `PATCH /sessions/{id}` — 更新会话标题或绑定模型。
async fn update_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<UpdateSessionBody>,
) -> Result<StatusCode, ApiError> {
    if let Some(model_id) = body.model_id.as_deref() {
        state.facade.set_model(&id, model_id).await?;
    }
    if body.title.is_some() || body.model_id.is_some() {
        state
            .meta
            .update_title_model(&id, body.title.as_deref(), body.model_id.as_deref())
            .await?;
    }
    if let Some(pr) = body.project_root {
        let raw = if pr.trim().is_empty() {
            None
        } else {
            Some(pr.as_str())
        };
        state.facade.set_project_root(&id, raw).await?;
        if let Err(e) = state.sync_filesystem_mcp().await {
            tracing::warn!("sync filesystem mcp after update project_root: {e}");
        }
    }
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /sessions/{id}` — 删除会话（含 adk session 与 memory 清理）。
async fn delete_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    state.facade.delete_session(&id).await?;
    if let Err(e) = state.sync_filesystem_mcp().await {
        tracing::warn!("sync filesystem mcp after delete session: {e}");
    }
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /models` — 列出模型配置（api_key 已脱敏）。
async fn list_models(State(state): State<AppState>) -> Result<Json<Vec<ModelView>>, ApiError> {
    Ok(Json(list_views(&state.models).await?))
}

/// `POST /models` — 新建模型配置。
async fn create_model(
    State(state): State<AppState>,
    Json(body): Json<ModelUpsertBody>,
) -> Result<Json<ModelView>, ApiError> {
    Ok(Json(upsert_from_body(&state.models, None, body).await?))
}

/// `PATCH /models/{id}` — 更新指定模型。
async fn update_model(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<ModelUpsertBody>,
) -> Result<Json<ModelView>, ApiError> {
    Ok(Json(upsert_from_body(&state.models, Some(&id), body).await?))
}

/// `DELETE /models/{id}` — 删除模型配置。
async fn delete_model(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    state.models.delete(&id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /chat` 请求体。
#[derive(Deserialize)]
struct ChatBody {
    /// 目标会话 ID。
    session_id: String,
    /// 用户输入消息文本。
    message: String,
    /// 本次请求覆盖的模型 ID（优先于会话绑定）。
    model_id: Option<String>,
}

/// `POST /chat` — 发起 Agent Run，以 SSE 流式返回事件。
async fn chat_sse(
    State(state): State<AppState>,
    Json(body): Json<ChatBody>,
) -> Result<impl IntoResponse, ApiError> {
    let model = state
        .facade
        .resolve_model(&state.models, &body.session_id, body.model_id.as_deref())
        .await?;

    let (_run_id, rx) = state
        .harness
        .run_chat(&body.session_id, &body.message, &model)
        .await?;
    let _ = state.meta.touch(&body.session_id).await;

    let stream = ReceiverStream::new(rx).map(|env| {
        let data = serde_json::to_string(&env).unwrap_or_else(|_| "{}".into());
        Ok::<_, std::convert::Infallible>(format!("data: {data}\n\n"))
    });

    Ok((
        StatusCode::OK,
        [
            (axum::http::header::CONTENT_TYPE, "text/event-stream"),
            (axum::http::header::CACHE_CONTROL, "no-cache"),
        ],
        axum::body::Body::from_stream(stream),
    ))
}

/// `POST /chat/{session_id}/interrupt` — 中断当前会话活跃的 Agent Run。
async fn interrupt_chat(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let run_id = state.harness.interrupt_session(&session_id).await?;
    Ok(Json(serde_json::json!({
        "session_id": session_id,
        "interrupted": run_id.is_some(),
        "run_id": run_id,
    })))
}

/// `GET /sessions/{id}/runs/active` — 查询会话当前活跃 Run ID（用于 SSE 重连）。
async fn get_active_run(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let run_id = state.harness.run_streams().active_run_id(&session_id).await;
    Ok(Json(serde_json::json!({
        "session_id": session_id,
        "run_id": run_id,
    })))
}

/// `GET /sessions/{id}/runs/{run_id}/stream` — 订阅活跃 Run 的 SSE 广播（断线重连）。
async fn stream_run(
    State(state): State<AppState>,
    Path((session_id, run_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, ApiError> {
    let (active_id, rx) = state
        .harness
        .run_streams()
        .subscribe(&session_id)
        .await
        .ok_or_else(|| MacoError::not_found("no active run for session"))?;
    if active_id != run_id {
        return Err(MacoError::not_found("run is not active").into());
    }

    let stream = BroadcastStream::new(rx).filter_map(|item: Result<maco_core::SseEnvelope, _>| {
        item.ok().map(|env| {
            let data = serde_json::to_string(&env).unwrap_or_else(|_| "{}".into());
            Ok::<_, std::convert::Infallible>(format!("data: {data}\n\n"))
        })
    });

    Ok((
        StatusCode::OK,
        [
            (axum::http::header::CONTENT_TYPE, "text/event-stream"),
            (axum::http::header::CACHE_CONTROL, "no-cache"),
        ],
        axum::body::Body::from_stream(stream),
    ))
}

/// `GET /sessions/{id}/runs/{run_id}` — 查询 Run 状态与挂起的工具/Elicitation。
async fn get_run(
    State(state): State<AppState>,
    Path((session_id, run_id)): Path<(String, String)>,
) -> Result<Json<RunStatusResponse>, ApiError> {
    let run = state
        .runs
        .get(&run_id)
        .await?
        .ok_or_else(|| MacoError::not_found("run"))?;
    if run.session_id != session_id {
        return Err(MacoError::not_found("run").into());
    }
    let pending_elicitations = load_pending_elicitations(&state, &session_id).await?;

    Ok(Json(RunStatusResponse {
        id: run.id,
        session_id: run.session_id,
        status: run.status,
        last_seq: run.last_seq as u64,
        pending_tools: pending_tools_from_resume(run.resume_context.as_deref()),
        pending_elicitations,
        error_message: run.error_message,
    }))
}

/// 从 DB 加载会话下所有 pending 状态的 Elicitation 摘要。
async fn load_pending_elicitations(
    state: &AppState,
    session_id: &str,
) -> Result<Vec<maco_core::PendingElicitation>, ApiError> {
    let records = state
        .elicitation
        .list_pending_for_session(session_id)
        .await?;
    Ok(records.iter().map(payload_summary).collect())
}

/// `GET /sessions/{id}/elicitation/pending` — 列出会话待处理的 Elicitation。
async fn list_pending_elicitation(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<Vec<maco_core::PendingElicitation>>, ApiError> {
    Ok(Json(load_pending_elicitations(&state, &session_id).await?))
}

/// `POST /elicitation/{id}/respond` — 用户对 MCP Elicitation 提交 accept/decline/cancel。
async fn respond_elicitation(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<ElicitationRespondBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let action = action_from_str(&body.action)
        .ok_or_else(|| MacoError::config("action must be accept, decline, or cancel"))?;
    let fulfilled = state
        .harness
        .respond_elicitation(&id, action, body.content)
        .await?;
    Ok(Json(serde_json::json!({
        "id": id,
        "fulfilled": fulfilled,
    })))
}

/// `POST /sessions/{id}/runs/{run_id}/resume` 请求体（HITL 工具审批恢复）。
#[derive(Deserialize)]
struct ResumeRunBody {
    /// 是否批准挂起的工具调用。
    approved: bool,
    /// 用户备注（可选，写入 resume 上下文）。
    note: Option<String>,
    /// 恢复时覆盖使用的模型 ID。
    model_id: Option<String>,
}

/// `POST /sessions/{id}/runs/{run_id}/resume` — HITL 批准后恢复 Run，SSE 流式返回。
async fn resume_run(
    State(state): State<AppState>,
    Path((session_id, run_id)): Path<(String, String)>,
    Json(body): Json<ResumeRunBody>,
) -> Result<impl IntoResponse, ApiError> {
    let model = state
        .facade
        .resolve_model(&state.models, &session_id, body.model_id.as_deref())
        .await?;

    let (_new_run_id, rx) = state
        .harness
        .resume_run(
            &session_id,
            &run_id,
            body.approved,
            body.note.as_deref(),
            &model,
        )
        .await?;

    let stream = ReceiverStream::new(rx).map(|env| {
        let data = serde_json::to_string(&env).unwrap_or_else(|_| "{}".into());
        Ok::<_, std::convert::Infallible>(format!("data: {data}\n\n"))
    });

    Ok((
        StatusCode::OK,
        [
            (axum::http::header::CONTENT_TYPE, "text/event-stream"),
            (axum::http::header::CACHE_CONTROL, "no-cache"),
        ],
        axum::body::Body::from_stream(stream),
    ))
}

/// `GET /sessions/{id}/plan` — 获取会话 ReAct 计划正文。
async fn get_plan(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<maco_db::PlanRecord>, ApiError> {
    let plan = state
        .react
        .get_plan(&id)
        .await?
        .ok_or_else(|| MacoError::not_found("plan"))?;
    Ok(Json(plan))
}

/// `PUT /sessions/{id}/plan` 请求体。
#[derive(Deserialize)]
struct PutPlanBody {
    /// 计划 Markdown/文本内容。
    content: String,
    /// 乐观锁版本号（可选，用于并发更新检测）。
    version: Option<i64>,
}

/// `PUT /sessions/{id}/plan` — 创建或更新 ReAct 计划。
async fn put_plan(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<PutPlanBody>,
) -> Result<Json<maco_db::PlanRecord>, ApiError> {
    Ok(Json(
        state
            .react
            .upsert_plan(&id, &body.content, body.version)
            .await?,
    ))
}

/// `GET /sessions/{id}/todos` — 列出会话下所有 Todo 项。
async fn list_todos(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Vec<maco_db::TodoRecord>>, ApiError> {
    Ok(Json(state.react.list_todos(&id).await?))
}

/// `PATCH /sessions/{id}/todos/{task_key}` 请求体。
#[derive(Deserialize)]
struct PatchTodoBody {
    /// 新状态（如 `pending` / `in_progress` / `done`）。
    status: String,
}

/// `PATCH /sessions/{id}/todos/{task_key}` — 更新单条 Todo 状态。
async fn patch_todo(
    State(state): State<AppState>,
    Path((id, task_key)): Path<(String, String)>,
    Json(body): Json<PatchTodoBody>,
) -> Result<Json<maco_db::TodoRecord>, ApiError> {
    Ok(Json(
        state
            .react
            .patch_todo_status(&id, &task_key, &body.status)
            .await?,
    ))
}

/// `GET /sessions/{id}/artifacts/{artifact_id}/preview` — 返回可预览的文本内容或元数据。
async fn preview_artifact(
    State(state): State<AppState>,
    Path((session_id, artifact_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    use maco_governance::is_previewable_mime;

    const PREVIEW_TEXT_LIMIT: usize = 512 * 1024;
    let (record, bytes) = state
        .artifacts
        .read(&session_id, &artifact_id)
        .await?;
    let previewable = is_previewable_mime(&record.mime_type);
    let kind = if record.mime_type.starts_with("image/") {
        "image"
    } else if record.mime_type.starts_with("text/")
        || record.mime_type == "application/json"
        || record.mime_type == "application/javascript"
        || record.mime_type == "application/xml"
    {
        "text"
    } else {
        "binary"
    };
    let mut content: Option<String> = None;
    let mut truncated = false;
    if previewable && kind == "text" {
        let text = String::from_utf8_lossy(&bytes);
        let limit = PREVIEW_TEXT_LIMIT;
        if text.len() > limit {
            content = Some(text[..limit].to_string());
            truncated = true;
        } else {
            content = Some(text.into_owned());
        }
    }
    Ok(Json(serde_json::json!({
        "id": record.id,
        "filename": record.filename,
        "mime_type": record.mime_type,
        "size_bytes": record.size_bytes,
        "previewable": previewable,
        "kind": kind,
        "content": content,
        "truncated": truncated,
    })))
}

/// `GET /sessions/{id}/artifacts/{artifact_id}` — 下载附件二进制。
async fn download_artifact(
    State(state): State<AppState>,
    Path((session_id, artifact_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, ApiError> {
    let (record, bytes) = state
        .artifacts
        .read(&session_id, &artifact_id)
        .await?;
    let disposition = format!(
        "attachment; filename=\"{}\"",
        record.filename.replace('"', "_")
    );
    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, record.mime_type),
            (header::CONTENT_DISPOSITION, disposition),
        ],
        bytes,
    ))
}

/// `GET /sessions/{id}/artifacts` — 列出会话已上传附件元数据。
async fn list_artifacts(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<Vec<maco_db::ArtifactRecord>>, ApiError> {
    Ok(Json(
        state
            .artifacts
            .repo()
            .list_for_session(&session_id)
            .await?,
    ))
}

/// `POST /sessions/{id}/artifacts` — multipart 上传附件（字段名 `file`）。
async fn upload_artifact(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    mut multipart: Multipart,
) -> Result<Json<maco_db::ArtifactRecord>, ApiError> {
    let mut filename = "upload.bin".to_string();
    let mut mime = "application/octet-stream".to_string();
    let mut bytes: Vec<u8> = Vec::new();

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| MacoError::config(e.to_string()))?
    {
        let name = field.name().unwrap_or_default().to_string();
        if name == "file" {
            filename = field.file_name().unwrap_or("upload.bin").to_string();
            mime = field
                .content_type()
                .map(str::to_string)
                .unwrap_or_else(|| "application/octet-stream".into());
            bytes = field
                .bytes()
                .await
                .map_err(|e| MacoError::config(e.to_string()))?
                .to_vec();
        }
    }

    Ok(Json(
        state
            .artifacts
            .save(&session_id, &filename, &mime, &bytes)
            .await?,
    ))
}

/// Memory 检索/删除的 query 参数。
#[derive(Deserialize)]
struct MemorySearchQuery {
    /// 搜索或删除匹配的关键词。
    q: String,
}

/// `GET /memory` 的 query 参数。
#[derive(Deserialize)]
struct MemoryListQuery {
    /// 返回条数上限，默认 50。
    #[serde(default = "default_memory_limit")]
    limit: usize,
}

fn default_memory_limit() -> usize {
    50
}

/// `POST /memory` 请求体。
#[derive(Deserialize)]
struct AddMemoryBody {
    /// 要写入全局 memory 的文本内容。
    content: String,
}

/// `GET /memory` — 分页列出 memory 条目。
async fn list_memory(
    State(state): State<AppState>,
    axum::extract::Query(q): axum::extract::Query<MemoryListQuery>,
) -> Result<Json<maco_core::MemoryListResponse>, ApiError> {
    Ok(Json(state.facade.memory_list(q.limit).await?))
}

/// `POST /memory` — 向全局 memory 追加一条记录。
async fn add_memory(
    State(state): State<AppState>,
    Json(body): Json<AddMemoryBody>,
) -> Result<StatusCode, ApiError> {
    if body.content.trim().is_empty() {
        return Err(MacoError::config("content must not be empty").into());
    }
    state.facade.memory_add(&body.content).await?;
    Ok(StatusCode::CREATED)
}

/// `DELETE /memory?q=...` — 按关键词删除 memory 条目。
async fn delete_memory(
    State(state): State<AppState>,
    axum::extract::Query(q): axum::extract::Query<MemorySearchQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if q.q.trim().is_empty() {
        return Err(MacoError::config("q query parameter required").into());
    }
    let deleted = state.facade.memory_delete(&q.q).await?;
    Ok(Json(serde_json::json!({ "deleted": deleted })))
}

/// `GET /memory/search?q=...` — 关键词检索 memory。
async fn memory_search(
    State(state): State<AppState>,
    axum::extract::Query(q): axum::extract::Query<MemorySearchQuery>,
) -> Result<Json<maco_core::MemorySearchResponse>, ApiError> {
    Ok(Json(state.facade.memory_search(&q.q).await?))
}

/// `GET /sessions/{id}/export` — 导出会话为 Markdown 附件下载。
async fn export_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let meta = state.meta.get(&id).await?;
    let plan = state.react.get_plan(&id).await?;
    let todos = state.react.list_todos(&id).await?;
    let md = session_markdown(&state.adk, meta.as_ref(), plan.as_ref(), &todos, &id).await?;
    let filename = format!("maco-session-{id}.md");
    let mut resp = axum::response::Response::new(md);
    *resp.status_mut() = StatusCode::OK;
    resp.headers_mut().insert(
        header::CONTENT_TYPE,
        "text/markdown; charset=utf-8".parse().unwrap(),
    );
    resp.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        format!("attachment; filename=\"{filename}\"")
            .parse()
            .unwrap(),
    );
    Ok(resp)
}

/// `GET /skills` — 扫描本地 Skill 目录并返回摘要列表。
async fn list_skills() -> Json<Vec<SkillSummary>> {
    Json(
        SkillLoader::scan(None)
            .into_iter()
            .map(|s| SkillSummary {
                name: s.name,
                description: s.description,
                file_path: s.file_path.display().to_string(),
            })
            .collect(),
    )
}

/// Skill 详情（含 Markdown 正文）。
#[derive(Serialize)]
struct SkillDetail {
    /// Skill 名称。
    name: String,
    /// 描述。
    description: String,
    /// 源文件路径。
    file_path: String,
    /// SKILL.md 正文。
    content: String,
}

/// `GET /skills/{name}` — 获取单个 Skill 的完整内容。
async fn get_skill(Path(name): Path<String>) -> Result<Json<SkillDetail>, ApiError> {
    let skill = SkillLoader::get(&name, None).ok_or_else(|| MacoError::not_found("skill"))?;
    Ok(Json(SkillDetail {
        name: skill.name,
        description: skill.description,
        file_path: skill.file_path.display().to_string(),
        content: skill.content,
    }))
}

/// `GET /usage/summary` 的 query 参数。
#[derive(Deserialize)]
struct UsageSummaryQuery {
    /// 统计起始时间（RFC3339，可选）。
    from: Option<String>,
    /// 统计结束时间（RFC3339，可选）。
    to: Option<String>,
    /// 聚合维度：`model` / `day` / `session`，默认 `model`。
    #[serde(default = "default_group_by")]
    group_by: String,
}

fn default_group_by() -> String {
    "model".into()
}

/// `GET /usage/summary` — 按维度汇总 token 用量与估算费用。
async fn usage_summary(
    State(state): State<AppState>,
    axum::extract::Query(q): axum::extract::Query<UsageSummaryQuery>,
) -> Result<Json<Vec<maco_db::UsageSummaryItem>>, ApiError> {
    let group_by = match q.group_by.as_str() {
        "model" | "day" | "session" => q.group_by.as_str(),
        _ => return Err(MacoError::config("group_by must be model, day, or session").into()),
    };
    Ok(Json(
        state
            .usage
            .summary(q.from.as_deref(), q.to.as_deref(), group_by)
            .await?,
    ))
}

/// `POST /auth/tokens` 请求体。
#[derive(Deserialize)]
struct CreateTokenBody {
    /// Token 显示名称（便于管理识别）。
    name: String,
    /// 授权 scope 列表，默认 `["admin", "*"]`。
    scopes: Option<Vec<String>>,
    /// 过期时间（RFC3339，可选；空表示不过期）。
    expires_at: Option<String>,
}

/// `POST /auth/tokens` 响应（明文 token 仅此次返回）。
#[derive(Serialize)]
struct CreateTokenResponse {
    /// Token 记录 ID。
    id: String,
    /// Token 名称。
    name: String,
    /// 明文 Bearer Token（请妥善保存，之后不可再查）。
    token: String,
    /// 生效的 scope 列表。
    scopes: Vec<String>,
    /// 创建时间。
    created_at: String,
}

/// `POST /auth/tokens` — 创建 API Token；首个 Token 可无鉴权创建。
async fn create_token(
    State(state): State<AppState>,
    auth: Option<Extension<AuthContext>>,
    Json(body): Json<CreateTokenBody>,
) -> Result<Json<CreateTokenResponse>, ApiError> {
    let count = state.api_tokens.count_enabled().await?;
    if count > 0 {
        let auth = auth.ok_or_else(|| MacoError::config("unauthorized"))?;
        require_admin(Some(&auth.0)).map_err(|_| MacoError::config("admin scope required"))?;
    }
    let scopes = body.scopes.unwrap_or_else(|| vec![SCOPE_ADMIN.into(), "*".into()]);
    let raw = generate_token();
    let hash = hash_token(&raw);
    let record = state
        .api_tokens
        .insert(&body.name, &hash, &scopes_json(&scopes), body.expires_at.as_deref())
        .await?;
    Ok(Json(CreateTokenResponse {
        id: record.id,
        name: record.name,
        token: raw,
        scopes,
        created_at: record.created_at,
    }))
}

/// `GET /auth/tokens` — 列出所有 Token（不含明文，需 admin scope）。
async fn list_tokens(
    State(state): State<AppState>,
    auth: Extension<AuthContext>,
) -> Result<Json<Vec<maco_db::ApiTokenListItem>>, ApiError> {
    require_admin(Some(&auth)).map_err(|_| MacoError::config("admin scope required"))?;
    Ok(Json(state.api_tokens.list().await?))
}

/// `POST /jobs` 请求体。
#[derive(Deserialize)]
struct CreateJobBody {
    /// 任务显示名称。
    name: String,
    /// 任务类型（如 `ping` / `log`）。
    job_type: String,
    /// 调度周期（`hourly` / `daily`，可选）。
    #[serde(default)]
    schedule: Option<String>,
    /// JSON 载荷字符串，默认 `{}`。
    #[serde(default)]
    payload: Option<String>,
    /// 首次执行时间（RFC3339，可选）。
    #[serde(default)]
    run_at: Option<String>,
}

/// `PATCH /jobs/{id}` 请求体。
#[derive(Deserialize)]
struct UpdateJobBody {
    /// 启用/禁用任务。
    #[serde(default)]
    enabled: Option<bool>,
}

/// `GET /jobs` — 列出所有后台任务。
async fn list_jobs(State(state): State<AppState>) -> Result<Json<Vec<JobRecord>>, ApiError> {
    Ok(Json(state.jobs.list().await?))
}

/// `POST /jobs` — 创建后台任务。
async fn create_job(
    State(state): State<AppState>,
    Json(body): Json<CreateJobBody>,
) -> Result<Json<JobRecord>, ApiError> {
    if body.name.trim().is_empty() {
        return Err(MacoError::config("name required").into());
    }
    let payload = body.payload.unwrap_or_else(|| "{}".into());
    let next = body.run_at.as_deref();
    Ok(Json(
        state
            .jobs
            .insert(
                body.name.trim(),
                body.job_type.trim(),
                body.schedule.as_deref(),
                &payload,
                next,
            )
            .await?,
    ))
}

/// `PATCH /jobs/{id}` — 更新任务（当前仅支持 enabled）。
async fn update_job(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<UpdateJobBody>,
) -> Result<StatusCode, ApiError> {
    if let Some(enabled) = body.enabled {
        state.jobs.set_enabled(&id, enabled).await?;
    }
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /jobs/{id}` — 删除任务。
async fn delete_job(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    if !state.jobs.delete(&id).await? {
        return Err(MacoError::not_found("job").into());
    }
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /jobs/{id}/run` — 立即执行一次任务（不等待调度）。
async fn run_job_now(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<JobRecord>, ApiError> {
    let job = state
        .jobs
        .get(&id)
        .await?
        .ok_or_else(|| MacoError::not_found("job"))?;
    state
        .jobs
        .update_run_result(&id, "running", None, None, None)
        .await?;
    let (status, result, err, next) = crate::worker::run_job_public(&job).await;
    state
        .jobs
        .update_run_result(&id, &status, result.as_deref(), err.as_deref(), next.as_deref())
        .await?;
    state
        .jobs
        .get(&id)
        .await?
        .ok_or_else(|| MacoError::not_found("job"))
        .map(Json)
        .map_err(Into::into)
}

/// `DELETE /auth/tokens/{id}` — 吊销 API Token（需 admin scope）。
async fn revoke_token(
    State(state): State<AppState>,
    auth: Extension<AuthContext>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    require_admin(Some(&auth)).map_err(|_| MacoError::config("admin scope required"))?;
    if !state.api_tokens.delete(&id).await? {
        return Err(MacoError::not_found("token").into());
    }
    Ok(StatusCode::NO_CONTENT)
}

/// `POST/PATCH /mcp/servers` 请求体。
#[derive(Deserialize)]
struct McpServerUpsertBody {
    /// 唯一服务名。
    name: String,
    /// 传输类型：`stdio` 或 `sse`。
    transport: String,
    /// stdio 命令（stdio 必填）。
    command: Option<String>,
    /// 命令参数 JSON 数组，默认 `[]`。
    #[serde(default)]
    args: Option<String>,
    /// SSE URL（sse 必填）。
    url: Option<String>,
    /// 环境变量 JSON 对象，默认 `{}`。
    #[serde(default)]
    env: Option<String>,
    /// 是否启用（PATCH 时可选）。
    #[serde(default)]
    enabled: Option<bool>,
}

/// `GET /mcp/servers` — 列出全部 MCP 服务配置。
async fn list_mcp_servers(
    State(state): State<AppState>,
) -> Result<Json<Vec<McpServerRecord>>, ApiError> {
    Ok(Json(state.mcp_servers.list().await?))
}

/// `POST /mcp/servers` — 创建 MCP 服务配置并重载连接池。
async fn create_mcp_server(
    State(state): State<AppState>,
    Json(body): Json<McpServerUpsertBody>,
) -> Result<Json<McpServerRecord>, ApiError> {
    validate_mcp_body(&body)?;
    let args = body.args.unwrap_or_else(|| "[]".into());
    let env = body.env.unwrap_or_else(|| "{}".into());
    let rec = state
        .mcp_servers
        .insert(
            body.name.trim(),
            body.transport.trim(),
            body.command.as_deref(),
            &args,
            body.url.as_deref(),
            &env,
        )
        .await?;
    if let Err(e) = state.mcp_pool.reload().await {
        tracing::warn!("mcp reload after create: {e}");
    }
    Ok(Json(rec))
}

/// `PATCH /mcp/servers/{id}` — 更新 MCP 配置并重载连接池。
async fn update_mcp_server(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<McpServerUpsertBody>,
) -> Result<Json<McpServerRecord>, ApiError> {
    validate_mcp_body(&body)?;
    let existing = state
        .mcp_servers
        .get(&id)
        .await?
        .ok_or_else(|| MacoError::not_found("mcp server"))?;
    let args = body.args.unwrap_or(existing.args);
    let env = body.env.unwrap_or(existing.env);
    let enabled = body.enabled.unwrap_or(existing.enabled != 0);
    state
        .mcp_servers
        .update(
            &id,
            body.name.trim(),
            body.transport.trim(),
            body.command.as_deref(),
            &args,
            body.url.as_deref(),
            &env,
            enabled,
        )
        .await?;
    if let Err(e) = state.mcp_pool.reload().await {
        tracing::warn!("mcp reload after update: {e}");
    }
    state
        .mcp_servers
        .get(&id)
        .await?
        .ok_or_else(|| MacoError::not_found("mcp server"))
        .map(Json)
        .map_err(Into::into)
}

/// `DELETE /mcp/servers/{id}` — 删除 MCP 配置并重载连接池。
async fn delete_mcp_server(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    if !state.mcp_servers.delete(&id).await? {
        return Err(MacoError::not_found("mcp server").into());
    }
    if let Err(e) = state.mcp_pool.reload().await {
        tracing::warn!("mcp reload after delete: {e}");
    }
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /mcp/reload` — 从 DB 重载 MCP 连接池。
async fn reload_mcp_pool(State(state): State<AppState>) -> Result<Json<serde_json::Value>, ApiError> {
    state.mcp_pool.reload().await?;
    let names = state.mcp_pool.status_summary().await;
    Ok(Json(serde_json::json!({ "reloaded": true, "servers": names })))
}

fn validate_mcp_body(body: &McpServerUpsertBody) -> Result<(), MacoError> {
    if body.name.trim().is_empty() {
        return Err(MacoError::config("name required"));
    }
    match body.transport.trim() {
        "stdio" => {
            if body
                .command
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .is_none()
            {
                return Err(MacoError::config("stdio transport requires command"));
            }
        }
        "sse" => {
            if body
                .url
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .is_none()
            {
                return Err(MacoError::config("sse transport requires url"));
            }
        }
        other => return Err(MacoError::config(format!("unsupported transport: {other}"))),
    }
    Ok(())
}

/// `POST/PATCH /tool-policies` 请求体。
#[derive(Deserialize)]
struct ToolPolicyUpsertBody {
    /// 工具名匹配模式（支持 `*` 通配）。
    tool_pattern: String,
    /// 来源：`tool` / `mcp` / `builtin`。
    source_type: String,
    /// 动作：`allow` / `confirm` / `deny`。
    action: String,
    /// 是否启用（PATCH 时可选）。
    #[serde(default)]
    enabled: Option<bool>,
}

async fn reload_harness_policies(state: &AppState) -> Result<(), MacoError> {
    let policies = state.tool_policies.list_enabled().await?;
    state.harness.set_tool_policies(policies).await;
    Ok(())
}

fn validate_policy_body(body: &ToolPolicyUpsertBody) -> Result<(), MacoError> {
    if body.tool_pattern.trim().is_empty() {
        return Err(MacoError::config("tool_pattern required"));
    }
    if body.source_type.trim().is_empty() {
        return Err(MacoError::config("source_type required"));
    }
    match body.action.trim() {
        "allow" | "confirm" | "deny" => Ok(()),
        other => Err(MacoError::config(format!("invalid action: {other}"))),
    }
}

/// `GET /tool-policies` — 列出全部 HITL 工具策略。
async fn list_tool_policies(
    State(state): State<AppState>,
) -> Result<Json<Vec<maco_db::ToolPolicyRecord>>, ApiError> {
    Ok(Json(state.tool_policies.list().await?))
}

/// `POST /tool-policies` — 新增策略并热更新 Harness。
async fn create_tool_policy(
    State(state): State<AppState>,
    Json(body): Json<ToolPolicyUpsertBody>,
) -> Result<Json<maco_db::ToolPolicyRecord>, ApiError> {
    validate_policy_body(&body)?;
    let rec = state
        .tool_policies
        .insert(
            body.tool_pattern.trim(),
            body.source_type.trim(),
            body.action.trim(),
        )
        .await?;
    reload_harness_policies(&state).await?;
    Ok(Json(rec))
}

/// `PATCH /tool-policies/{id}` — 更新策略并热更新 Harness。
async fn update_tool_policy(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<ToolPolicyUpsertBody>,
) -> Result<Json<maco_db::ToolPolicyRecord>, ApiError> {
    validate_policy_body(&body)?;
    let existing = state
        .tool_policies
        .get(&id)
        .await?
        .ok_or_else(|| MacoError::not_found("tool policy"))?;
    let enabled = body.enabled.unwrap_or(existing.enabled != 0);
    state
        .tool_policies
        .update(
            &id,
            body.tool_pattern.trim(),
            body.source_type.trim(),
            body.action.trim(),
            enabled,
        )
        .await?;
    reload_harness_policies(&state).await?;
    state
        .tool_policies
        .get(&id)
        .await?
        .ok_or_else(|| MacoError::not_found("tool policy"))
        .map(Json)
        .map_err(Into::into)
}

/// `DELETE /tool-policies/{id}` — 删除策略并热更新 Harness。
async fn delete_tool_policy(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    if !state.tool_policies.delete(&id).await? {
        return Err(MacoError::not_found("tool policy").into());
    }
    reload_harness_policies(&state).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /tool-policies/reload` — 从 DB 重载策略到 Harness。
async fn reload_tool_policies(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    reload_harness_policies(&state).await?;
    let count = state.tool_policies.list_enabled().await?.len();
    Ok(Json(serde_json::json!({ "reloaded": true, "enabled_count": count })))
}

/// Skill 扫描结果摘要（`GET /skills` 响应项）。
#[derive(Serialize)]
struct SkillSummary {
    /// Skill 名称（来自 frontmatter）。
    name: String,
    /// Skill 描述。
    description: String,
    /// SKILL.md 文件绝对路径。
    file_path: String,
}

/// 将 `MacoError` 映射为 HTTP 状态码的包装类型。
struct ApiError(MacoError);

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
