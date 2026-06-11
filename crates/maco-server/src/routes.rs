use axum::{
    extract::{Extension, Multipart, Path, State},
    http::{header, StatusCode},
    response::IntoResponse,
    routing::{delete, get, patch, post},
    Json, Router,
};
use maco_core::{pending_tools_from_resume, MacoError, RunStatusResponse};
use maco_db::payload_summary;
use maco_harness::elicitation::{action_from_str, ElicitationRespondBody};
use maco_db::JobRecord;
use maco_governance::{
    generate_token, hash_token, pii_guardrail_enabled, scopes_json, SCOPE_ADMIN,
};
use maco_harness::SkillLoader;
use serde::{Deserialize, Serialize};
use tokio_stream::{wrappers::ReceiverStream, StreamExt as _};

use crate::auth::{require_admin, AuthContext};
use crate::export::session_markdown;
use crate::models_api::{list_views, upsert_from_body, ModelUpsertBody, ModelView};
use crate::AppState;

pub fn api_router() -> Router<AppState> {
    Router::new()
        .route("/health", get(health))
        .route("/guardrail/status", get(guardrail_status))
        .route("/sessions", get(list_sessions).post(create_session))
        .route("/sessions/{id}", patch(update_session).delete(delete_session))
        .route("/sessions/{id}/plan", get(get_plan).put(put_plan))
        .route("/sessions/{id}/todos", get(list_todos))
        .route("/sessions/{id}/todos/{task_key}", patch(patch_todo))
        .route("/sessions/{id}/artifacts", post(upload_artifact))
        .route("/sessions/{id}/export", get(export_session))
        .route("/sessions/{id}/runs/{run_id}", get(get_run))
        .route("/sessions/{id}/runs/{run_id}/resume", post(resume_run))
        .route("/sessions/{id}/elicitation/pending", get(list_pending_elicitation))
        .route("/elicitation/{id}/respond", post(respond_elicitation))
        .route("/models", get(list_models).post(create_model))
        .route("/models/{id}", patch(update_model).delete(delete_model))
        .route("/chat", post(chat_sse))
        .route("/chat/{session_id}/interrupt", post(interrupt_chat))
        .route("/memory", get(list_memory).post(add_memory).delete(delete_memory))
        .route("/memory/search", get(memory_search))
        .route("/skills", get(list_skills))
        .route("/auth/tokens", get(list_tokens).post(create_token))
        .route("/auth/tokens/{id}", delete(revoke_token))
        .route("/usage/summary", get(usage_summary))
        .route("/jobs", get(list_jobs).post(create_job))
        .route("/jobs/{id}", patch(update_job).delete(delete_job))
        .route("/jobs/{id}/run", post(run_job_now))
}

async fn guardrail_status() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "pii_enabled": pii_guardrail_enabled(),
        "log_redact": std::env::var("MACO_LOG_REDACT").unwrap_or_else(|_| "basic".into()),
    }))
}

async fn health(State(state): State<AppState>) -> impl IntoResponse {
    let _guard = state.mcp_pool.acquire("health-check").await;
    Json(serde_json::json!({
        "db": "ok",
        "mcp": ["pool_ready"],
        "memory": "ok",
        "skills": SkillLoader::scan(None).len(),
        "bind": state.bind_addr,
    }))
}

async fn list_sessions(State(state): State<AppState>) -> Result<Json<Vec<maco_db::SessionMetaRecord>>, ApiError> {
    Ok(Json(state.facade.list_sessions().await?))
}

#[derive(Deserialize)]
struct CreateSessionBody {
    title: Option<String>,
    model_id: Option<String>,
}

async fn create_session(
    State(state): State<AppState>,
    Json(body): Json<CreateSessionBody>,
) -> Result<Json<maco_db::SessionMetaRecord>, ApiError> {
    Ok(Json(
        state
            .facade
            .create_session(body.title, body.model_id)
            .await?,
    ))
}

#[derive(Deserialize)]
struct UpdateSessionBody {
    title: Option<String>,
    model_id: Option<String>,
}

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
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    state.facade.delete_session(&id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_models(State(state): State<AppState>) -> Result<Json<Vec<ModelView>>, ApiError> {
    Ok(Json(list_views(&state.models).await?))
}

async fn create_model(
    State(state): State<AppState>,
    Json(body): Json<ModelUpsertBody>,
) -> Result<Json<ModelView>, ApiError> {
    Ok(Json(upsert_from_body(&state.models, None, body).await?))
}

async fn update_model(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<ModelUpsertBody>,
) -> Result<Json<ModelView>, ApiError> {
    Ok(Json(upsert_from_body(&state.models, Some(&id), body).await?))
}

async fn delete_model(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    state.models.delete(&id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct ChatBody {
    session_id: String,
    message: String,
    model_id: Option<String>,
}

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

async fn interrupt_chat(
    Path(session_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(serde_json::json!({ "session_id": session_id, "interrupted": true })))
}

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

async fn list_pending_elicitation(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<Vec<maco_core::PendingElicitation>>, ApiError> {
    Ok(Json(load_pending_elicitations(&state, &session_id).await?))
}

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

#[derive(Deserialize)]
struct ResumeRunBody {
    approved: bool,
    note: Option<String>,
    model_id: Option<String>,
}

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

#[derive(Deserialize)]
struct PutPlanBody {
    content: String,
    version: Option<i64>,
}

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

async fn list_todos(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Vec<maco_db::TodoRecord>>, ApiError> {
    Ok(Json(state.react.list_todos(&id).await?))
}

#[derive(Deserialize)]
struct PatchTodoBody {
    status: String,
}

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

#[derive(Deserialize)]
struct MemorySearchQuery {
    q: String,
}

#[derive(Deserialize)]
struct MemoryListQuery {
    #[serde(default = "default_memory_limit")]
    limit: usize,
}

fn default_memory_limit() -> usize {
    50
}

#[derive(Deserialize)]
struct AddMemoryBody {
    content: String,
}

async fn list_memory(
    State(state): State<AppState>,
    axum::extract::Query(q): axum::extract::Query<MemoryListQuery>,
) -> Result<Json<maco_core::MemoryListResponse>, ApiError> {
    Ok(Json(state.facade.memory_list(q.limit).await?))
}

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

async fn memory_search(
    State(state): State<AppState>,
    axum::extract::Query(q): axum::extract::Query<MemorySearchQuery>,
) -> Result<Json<maco_core::MemorySearchResponse>, ApiError> {
    Ok(Json(state.facade.memory_search(&q.q).await?))
}

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

#[derive(Deserialize)]
struct UsageSummaryQuery {
    from: Option<String>,
    to: Option<String>,
    #[serde(default = "default_group_by")]
    group_by: String,
}

fn default_group_by() -> String {
    "model".into()
}

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

#[derive(Deserialize)]
struct CreateTokenBody {
    name: String,
    scopes: Option<Vec<String>>,
    expires_at: Option<String>,
}

#[derive(Serialize)]
struct CreateTokenResponse {
    id: String,
    name: String,
    token: String,
    scopes: Vec<String>,
    created_at: String,
}

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

async fn list_tokens(
    State(state): State<AppState>,
    auth: Extension<AuthContext>,
) -> Result<Json<Vec<maco_db::ApiTokenListItem>>, ApiError> {
    require_admin(Some(&auth)).map_err(|_| MacoError::config("admin scope required"))?;
    Ok(Json(state.api_tokens.list().await?))
}

#[derive(Deserialize)]
struct CreateJobBody {
    name: String,
    job_type: String,
    #[serde(default)]
    schedule: Option<String>,
    #[serde(default)]
    payload: Option<String>,
    #[serde(default)]
    run_at: Option<String>,
}

#[derive(Deserialize)]
struct UpdateJobBody {
    #[serde(default)]
    enabled: Option<bool>,
}

async fn list_jobs(State(state): State<AppState>) -> Result<Json<Vec<JobRecord>>, ApiError> {
    Ok(Json(state.jobs.list().await?))
}

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

async fn delete_job(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    if !state.jobs.delete(&id).await? {
        return Err(MacoError::not_found("job").into());
    }
    Ok(StatusCode::NO_CONTENT)
}

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

#[derive(Serialize)]
struct SkillSummary {
    name: String,
    description: String,
    file_path: String,
}

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
