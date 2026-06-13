//! Chat SSE 与 Run interrupt 路由。

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::post,
};
use serde::Deserialize;
use tokio_stream::{StreamExt as _, wrappers::ReceiverStream};

use crate::AppState;
use crate::routes::ApiError;

/// Chat 路由，挂载于 `/api` 下。
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/chat", post(chat_sse))
        .route("/chat/{session_id}/interrupt", post(interrupt_chat))
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
        .agent
        .facade
        .resolve_model(
            &state.repos.models,
            &body.session_id,
            body.model_id.as_deref(),
        )
        .await?;

    let (_run_id, rx) = state
        .agent
        .harness
        .run_chat(&body.session_id, &body.message, &model)
        .await?;
    let _ = state.repos.meta.touch(&body.session_id).await;

    let stream = ReceiverStream::new(rx).map(|env| {
        let data = serde_json::to_string(&env).unwrap_or_else(|e| {
            tracing::warn!("failed to serialize chat SSE envelope: {e}");
            "{}".into()
        });
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
    let run_id = state.agent.harness.interrupt_session(&session_id).await?;
    Ok(Json(serde_json::json!({
        "session_id": session_id,
        "interrupted": run_id.is_some(),
        "run_id": run_id,
    })))
}
