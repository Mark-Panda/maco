//! 会话 CRUD、worktree、ReAct plan/todo 与导出路由。

use axum::{
    Json, Router,
    extract::{Path, State},
    http::{StatusCode, header},
    response::IntoResponse,
    routing::{get, patch, post},
};
use maco_core::MacoError;
use serde::Deserialize;

use crate::AppState;
use crate::export::session_markdown;
use crate::routes::ApiError;

/// 会话相关路由，挂载于 `/api` 下。
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/sessions", get(list_sessions).post(create_session))
        .route(
            "/sessions/{id}",
            patch(update_session).delete(delete_session),
        )
        .route("/sessions/{id}/messages", get(get_session_messages))
        .route("/sessions/{id}/plan", get(get_plan).put(put_plan))
        .route("/sessions/{id}/todos", get(list_todos))
        .route("/sessions/{id}/todos/{task_key}", patch(patch_todo))
        .route("/sessions/{id}/export", get(export_session))
        .route("/worktree/status", get(get_worktree_status))
        .route(
            "/sessions/{id}/worktree/provision",
            post(provision_session_worktree),
        )
}

/// `GET /sessions` — 列出所有会话元数据（与 adk session 对齐）。
async fn list_sessions(
    State(state): State<AppState>,
) -> Result<Json<Vec<crate::session_meta_view::SessionMetaView>>, ApiError> {
    let rows = state.agent.facade.list_sessions().await?;
    Ok(Json(crate::session_meta_view::enrich_sessions(rows)))
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
    /// Agent 权限模式，默认 `request_approval`。
    permission_mode: Option<String>,
    /// 是否强制 Git worktree，默认 `true`。
    git_worktree_enabled: Option<bool>,
    /// worktree 分支前缀，默认 `maco/agent`。
    git_branch_prefix: Option<String>,
}

/// `POST /sessions` — 创建新会话。
async fn create_session(
    State(state): State<AppState>,
    Json(body): Json<CreateSessionBody>,
) -> Result<Json<crate::session_meta_view::SessionMetaView>, ApiError> {
    let permission_mode = body
        .permission_mode
        .as_deref()
        .map(maco_core::AgentPermissionMode::parse);
    let rec = state
        .agent
        .facade
        .create_session(
            body.title,
            body.model_id,
            body.project_root,
            permission_mode,
            body.git_worktree_enabled,
            body.git_branch_prefix,
        )
        .await?;
    Ok(Json(
        crate::session_meta_view::SessionMetaView::from_record(rec),
    ))
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
    /// Agent 权限模式。
    permission_mode: Option<String>,
    /// 是否强制 Git worktree。
    git_worktree_enabled: Option<bool>,
    /// worktree 分支前缀。
    git_branch_prefix: Option<String>,
}

/// `GET /sessions/{id}/messages` — 加载会话历史消息（供前端恢复对话）。
async fn get_session_messages(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<maco_core::SessionMessagesResponse>, ApiError> {
    Ok(Json(state.agent.facade.session_messages(&id).await?))
}

/// `PATCH /sessions/{id}` — 更新会话标题或绑定模型。
async fn update_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<UpdateSessionBody>,
) -> Result<StatusCode, ApiError> {
    if let Some(model_id) = body.model_id.as_deref() {
        state.agent.facade.set_model(&id, model_id).await?;
    }
    if body.title.is_some() || body.model_id.is_some() {
        state
            .repos
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
        state.agent.facade.set_project_root(&id, raw).await?;
        state.invalidate_session_filesystem_cache(&id).await;
    }
    if let Some(mode) = body.permission_mode.as_deref() {
        state
            .agent
            .facade
            .set_permission_mode(&id, maco_core::AgentPermissionMode::parse(mode))
            .await?;
    }
    if body.git_worktree_enabled.is_some() || body.git_branch_prefix.is_some() {
        let rec = state
            .repos
            .meta
            .get(&id)
            .await?
            .ok_or_else(|| ApiError::from(MacoError::not_found("session not found")))?;
        let enabled = body
            .git_worktree_enabled
            .unwrap_or(rec.git_worktree_enabled != 0);
        let prefix = body
            .git_branch_prefix
            .as_deref()
            .unwrap_or(rec.git_branch_prefix.as_str());
        state
            .agent
            .facade
            .set_git_worktree_settings(&id, enabled, prefix)
            .await?;
        state.invalidate_session_filesystem_cache(&id).await;
    }
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /sessions/{id}` — 删除会话（含 adk session 与 memory 清理）。
async fn delete_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    state.invalidate_session_filesystem_cache(&id).await;
    state.agent.facade.delete_session(&id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /worktree/status` 查询参数。
#[derive(Deserialize)]
struct WorktreeStatusQuery {
    /// 项目根目录（绝对路径）。
    project_root: String,
    /// 是否启用 worktree（`1`/`0` 或 `true`/`false`），默认 `true`。
    enabled: Option<String>,
}

/// `GET /worktree/status` — 探测项目路径的 Git worktree 状态（无需会话）。
async fn get_worktree_status(
    axum::extract::Query(q): axum::extract::Query<WorktreeStatusQuery>,
) -> Json<serde_json::Value> {
    let enabled = !matches!(
        q.enabled.as_deref().map(str::trim),
        Some("0") | Some("false") | Some("off")
    );
    let status = maco_core::git_worktree_status(enabled, Some(q.project_root.as_str()), None);
    Json(serde_json::json!({ "status": status }))
}

/// `POST /sessions/{id}/worktree/provision` — 手动重试 Git worktree provision。
async fn provision_session_worktree(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<crate::session_meta_view::SessionMetaView>, ApiError> {
    state.agent.facade.provision_worktree(&session_id).await?;
    state.invalidate_session_filesystem_cache(&session_id).await;
    let rec = state
        .repos
        .meta
        .get(&session_id)
        .await?
        .ok_or_else(|| MacoError::not_found("session"))?;
    Ok(Json(
        crate::session_meta_view::SessionMetaView::from_record(rec),
    ))
}

/// `GET /sessions/{id}/plan` — 获取会话 ReAct 计划正文。
async fn get_plan(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<maco_db::PlanRecord>, ApiError> {
    let plan = state
        .repos
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
            .repos
            .react
            .upsert_plan(&id, &body.content, body.version)
            .await?,
    ))
}

/// `GET /sessions/{id}/todos` — 列出会话下所有 Todo 项（只读；plan 同步仅在 `update_plan` 时触发）。
async fn list_todos(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Vec<maco_db::TodoRecord>>, ApiError> {
    Ok(Json(state.repos.react.list_todos(&id).await?))
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
            .repos
            .react
            .patch_todo_status(&id, &task_key, &body.status)
            .await?,
    ))
}

/// `GET /sessions/{id}/export` — 导出会话为 Markdown 附件下载。
async fn export_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let meta = state.repos.meta.get(&id).await?;
    let plan = state.repos.react.get_plan(&id).await?;
    let todos = state.repos.react.list_todos(&id).await?;
    let md = session_markdown(&state.agent.adk, meta.as_ref(), plan.as_ref(), &todos, &id).await?;
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
