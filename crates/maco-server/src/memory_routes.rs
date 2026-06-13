//! 全局 Memory 管理路由。

use axum::{Json, Router, extract::State, http::StatusCode, routing::get};
use maco_core::MacoError;
use serde::Deserialize;

use crate::AppState;
use crate::routes::ApiError;

/// Memory 路由，挂载于 `/api` 下。
pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/memory",
            get(list_memory).post(add_memory).delete(delete_memory),
        )
        .route("/memory/search", get(memory_search))
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
    Ok(Json(state.agent.facade.memory_list(q.limit).await?))
}

/// `POST /memory` — 向全局 memory 追加一条记录。
async fn add_memory(
    State(state): State<AppState>,
    Json(body): Json<AddMemoryBody>,
) -> Result<StatusCode, ApiError> {
    if body.content.trim().is_empty() {
        return Err(MacoError::config("content must not be empty").into());
    }
    state.agent.facade.memory_add(&body.content).await?;
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
    let deleted = state.agent.facade.memory_delete(&q.q).await?;
    Ok(Json(serde_json::json!({ "deleted": deleted })))
}

/// `GET /memory/search?q=...` — 关键词检索 memory。
async fn memory_search(
    State(state): State<AppState>,
    axum::extract::Query(q): axum::extract::Query<MemorySearchQuery>,
) -> Result<Json<maco_core::MemorySearchResponse>, ApiError> {
    Ok(Json(state.agent.facade.memory_search(&q.q).await?))
}
