//! Run / SSE / HITL / Elicitation / sub-agent HTTP routes.

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use maco_core::{
    MacoError, RUN_STATUS_AWAITING_USER, RUN_STATUS_RUNNING, RunStatusResponse,
    pending_tools_from_resume,
};
use maco_db::payload_summary;
use maco_harness::elicitation::{ElicitationRespondBody, action_from_str};
use serde::Deserialize;
use tokio_stream::{StreamExt as _, wrappers::BroadcastStream, wrappers::ReceiverStream};

use crate::{AppState, routes::ApiError};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/sessions/{id}/runs/active", get(get_active_run))
        .route("/sessions/{id}/runs/{run_id}", get(get_run))
        .route("/sessions/{id}/runs/{run_id}/stream", get(stream_run))
        .route("/sessions/{id}/runs/{run_id}/resume", post(resume_run))
        .route("/sessions/{id}/sub-agent-runs", get(list_sub_agent_runs))
        .route(
            "/sessions/{id}/sub-agent-runs/{sub_run_id}",
            get(get_sub_agent_run),
        )
        .route(
            "/sessions/{id}/runs/{run_id}/sub-agents/{task_key}/cancel",
            post(cancel_sub_agent),
        )
        .route(
            "/sessions/{id}/elicitation/pending",
            get(list_pending_elicitation),
        )
        .route("/elicitation/{id}/respond", post(respond_elicitation))
}

/// `GET /sessions/{id}/runs/active` — 查询会话当前活跃 Run（内存流优先，否则查 DB）。
async fn get_active_run(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if let Some(run_id) = state
        .agent
        .harness
        .run_streams()
        .active_run_id(&session_id)
        .await
    {
        return Ok(Json(serde_json::json!({
            "session_id": session_id,
            "run_id": run_id,
            "source": "stream",
        })));
    }

    if let Some(run) = state
        .repos
        .runs
        .find_active_for_session(&session_id)
        .await?
    {
        return Ok(Json(serde_json::json!({
            "session_id": session_id,
            "run_id": run.id,
            "status": run.status,
            "source": "db",
        })));
    }

    Ok(Json(serde_json::json!({
        "session_id": session_id,
        "run_id": null,
    })))
}

/// `GET /sessions/{id}/runs/{run_id}/stream` — 订阅活跃 Run 的 SSE 广播（断线重连）。
#[derive(Deserialize)]
struct StreamRunQuery {
    /// 已收到的最后 SSE 事件序号；传入后会回放内存中更晚的事件。
    after_seq: Option<u64>,
}

async fn stream_run(
    State(state): State<AppState>,
    Path((session_id, run_id)): Path<(String, String)>,
    Query(q): Query<StreamRunQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let Some(sub) = state
        .agent
        .harness
        .run_streams()
        .subscribe_since(&session_id, q.after_seq)
        .await
    else {
        let run = state
            .repos
            .runs
            .get(&run_id)
            .await?
            .ok_or_else(|| MacoError::not_found("run"))?;
        if run.session_id != session_id {
            return Err(MacoError::not_found("run").into());
        }
        let replay = load_run_replay_with_terminal_marker(&state, &run, q.after_seq).await?;
        let stream = tokio_stream::iter(
            replay
                .into_iter()
                .map(|env| Ok::<_, std::convert::Infallible>(sse_data(&env))),
        );
        return Ok((
            StatusCode::OK,
            [
                (axum::http::header::CONTENT_TYPE, "text/event-stream"),
                (axum::http::header::CACHE_CONTROL, "no-cache"),
            ],
            axum::body::Body::from_stream(stream),
        ));
    };
    if sub.session_id != session_id || sub.run_id != run_id {
        return Err(MacoError::not_found("run is not active").into());
    }

    let mut replay_events = sub.replay;
    if sub.replay_gap {
        replay_events.insert(0, stream_gap_envelope(&run_id, q.after_seq));
    }
    let replay = tokio_stream::iter(
        replay_events
            .into_iter()
            .map(|env| Ok::<_, std::convert::Infallible>(sse_data(&env))),
    );
    let live =
        BroadcastStream::new(sub.rx).filter_map(|item: Result<maco_core::SseEnvelope, _>| {
            item.ok()
                .map(|env| Ok::<_, std::convert::Infallible>(sse_data(&env)))
        });
    let stream = replay.chain(live);

    Ok((
        StatusCode::OK,
        [
            (axum::http::header::CONTENT_TYPE, "text/event-stream"),
            (axum::http::header::CACHE_CONTROL, "no-cache"),
        ],
        axum::body::Body::from_stream(stream),
    ))
}

fn sse_data(env: &maco_core::SseEnvelope) -> String {
    let data = serde_json::to_string(env).unwrap_or_else(|e| {
        tracing::warn!("failed to serialize SSE envelope: {e}");
        "{}".into()
    });
    format!("data: {data}\n\n")
}

fn is_terminal_run_status(status: &str) -> bool {
    status != RUN_STATUS_RUNNING && status != RUN_STATUS_AWAITING_USER
}

async fn load_run_replay_with_terminal_marker(
    state: &AppState,
    run: &maco_db::RunRecord,
    after_seq: Option<u64>,
) -> Result<Vec<maco_core::SseEnvelope>, ApiError> {
    const PAGE_SIZE: u32 = 1_000;
    const MAX_REPLAY_EVENTS: usize = 10_000;

    let mut cursor = after_seq;
    let mut replay = Vec::new();
    loop {
        let page = state
            .repos
            .run_events
            .list_after(&run.id, cursor, PAGE_SIZE)
            .await?;
        if page.is_empty() {
            break;
        }
        cursor = page.last().map(|env| env.seq);
        replay.extend(page);
        if replay.len() >= MAX_REPLAY_EVENTS || replay.len() % PAGE_SIZE as usize != 0 {
            break;
        }
    }

    if is_terminal_run_status(&run.status) || replay.is_empty() {
        let marker = stream_ended_envelope(run, after_seq, replay.last().map(|env| env.seq));
        replay.push(marker);
    }
    Ok(replay)
}

fn stream_ended_envelope(
    run: &maco_db::RunRecord,
    after_seq: Option<u64>,
    last_replayed_seq: Option<u64>,
) -> maco_core::SseEnvelope {
    let last_seq = run.last_seq.max(0) as u64;
    let gap = last_replayed_seq.or(after_seq).unwrap_or(last_seq) < last_seq;
    maco_core::SseEnvelope {
        event_type: if is_terminal_run_status(&run.status) {
            "stream_ended".into()
        } else {
            "stream_unavailable".into()
        },
        run_id: run.id.clone(),
        seq: last_seq.saturating_add(1),
        payload: serde_json::json!({
            "status": run.status,
            "last_seq": run.last_seq,
            "gap": gap,
            "last_replayed_seq": last_replayed_seq,
            "replay_available": last_replayed_seq.is_some() || last_seq > 0,
            "live_stream": !is_terminal_run_status(&run.status),
        }),
    }
}

fn stream_gap_envelope(run_id: &str, after_seq: Option<u64>) -> maco_core::SseEnvelope {
    maco_core::SseEnvelope {
        event_type: "stream_gap".into(),
        run_id: run_id.to_string(),
        seq: after_seq.unwrap_or(0).saturating_add(1),
        payload: serde_json::json!({
            "after_seq": after_seq,
            "gap": true,
            "message": "some replay events were unavailable",
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_record(status: &str) -> maco_db::RunRecord {
        maco_db::RunRecord {
            id: "run-1".into(),
            session_id: "session-1".into(),
            status: status.into(),
            resume_context: None,
            superseded_by: None,
            error_message: None,
            last_seq: 7,
            created_at: "now".into(),
            updated_at: "now".into(),
        }
    }

    #[test]
    fn terminal_status_excludes_live_run_states() {
        assert!(!is_terminal_run_status(RUN_STATUS_RUNNING));
        assert!(!is_terminal_run_status(RUN_STATUS_AWAITING_USER));
        assert!(is_terminal_run_status("completed"));
        assert!(is_terminal_run_status("failed"));
    }

    #[test]
    fn stream_ended_envelope_advances_after_seq() {
        let env = stream_ended_envelope(&run_record("completed"), Some(10), None);
        assert_eq!(env.event_type, "stream_ended");
        assert_eq!(env.run_id, "run-1");
        assert_eq!(env.seq, 8);
        assert_eq!(env.payload["status"], "completed");
        assert_eq!(env.payload["last_seq"], 7);
        assert_eq!(env.payload["gap"], false);
    }
}

/// `GET /sessions/{id}/runs/{run_id}` — 查询 Run 状态与挂起的工具/Elicitation。
async fn get_run(
    State(state): State<AppState>,
    Path((session_id, run_id)): Path<(String, String)>,
) -> Result<Json<RunStatusResponse>, ApiError> {
    let run = state
        .repos
        .runs
        .get(&run_id)
        .await?
        .ok_or_else(|| MacoError::not_found("run"))?;
    if run.session_id != session_id {
        return Err(MacoError::not_found("run").into());
    }
    let pending_elicitations = load_pending_elicitations_for_run(&state, &run_id).await?;

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

/// `GET /sessions/{id}/sub-agent-runs` 查询参数。
#[derive(Deserialize)]
struct ListSubAgentRunsQuery {
    task_key: Option<String>,
    limit: Option<u32>,
}

/// `GET /sessions/{id}/sub-agent-runs` — 子 Agent spawn 审计列表。
async fn list_sub_agent_runs(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Query(q): Query<ListSubAgentRunsQuery>,
) -> Result<Json<Vec<maco_db::SubAgentRunRecord>>, ApiError> {
    let limit = q.limit.unwrap_or(50);
    let task_key = q.task_key.as_deref();
    Ok(Json(
        state
            .repos
            .sub_agent_runs
            .list_for_session(&session_id, task_key, limit)
            .await?,
    ))
}

/// `GET /sessions/{id}/sub-agent-runs/{sub_run_id}` — 单条子 Agent 审计详情。
async fn get_sub_agent_run(
    State(state): State<AppState>,
    Path((session_id, sub_run_id)): Path<(String, String)>,
) -> Result<Json<maco_db::SubAgentRunRecord>, ApiError> {
    let rec = state
        .repos
        .sub_agent_runs
        .get(&sub_run_id)
        .await?
        .ok_or_else(|| MacoError::not_found("sub_agent_run"))?;
    if rec.session_id != session_id {
        return Err(MacoError::not_found("sub_agent_run").into());
    }
    Ok(Json(rec))
}

/// `POST /sessions/{id}/runs/{run_id}/sub-agents/{task_key}/cancel` — 取消活跃子 Agent。
async fn cancel_sub_agent(
    State(state): State<AppState>,
    Path((session_id, run_id, task_key)): Path<(String, String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let cancelled = state
        .agent
        .harness
        .cancel_sub_agent(&session_id, &run_id, &task_key)
        .await?;
    Ok(Json(serde_json::json!({ "cancelled": cancelled })))
}

/// 从 DB 加载会话下所有 pending 状态的 Elicitation 摘要。
async fn load_pending_elicitations(
    state: &AppState,
    session_id: &str,
) -> Result<Vec<maco_core::PendingElicitation>, ApiError> {
    let records = state
        .repos
        .elicitation
        .list_pending_for_session(session_id)
        .await?;
    Ok(records.iter().map(payload_summary).collect())
}

/// 从 DB 加载指定 Run 的 pending Elicitation（避免历史残留干扰重连）。
async fn load_pending_elicitations_for_run(
    state: &AppState,
    run_id: &str,
) -> Result<Vec<maco_core::PendingElicitation>, ApiError> {
    let records = state.repos.elicitation.list_pending_for_run(run_id).await?;
    Ok(records.iter().map(payload_summary).collect())
}

/// `GET /sessions/{id}/elicitation/pending` — 列出待处理 Elicitation（优先按活跃 Run 过滤）。
async fn list_pending_elicitation(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<Vec<maco_core::PendingElicitation>>, ApiError> {
    if let Some(run) = state
        .repos
        .runs
        .find_active_for_session(&session_id)
        .await?
    {
        return Ok(Json(
            load_pending_elicitations_for_run(&state, &run.id).await?,
        ));
    }
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
        .agent
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

/// `POST /sessions/{id}/runs/{run_id}/resume` — HITL 批准后恢复 Run。
async fn resume_run(
    State(state): State<AppState>,
    Path((session_id, run_id)): Path<(String, String)>,
    Json(body): Json<ResumeRunBody>,
) -> Result<impl IntoResponse, ApiError> {
    let model = state
        .agent
        .facade
        .resolve_model(&state.repos.models, &session_id, body.model_id.as_deref())
        .await?;

    match state
        .agent
        .harness
        .resume_run(
            &session_id,
            &run_id,
            body.approved,
            body.note.as_deref(),
            &model,
        )
        .await?
    {
        maco_harness::ResumeHitlOutcome::InPlace => Ok((
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "application/json")],
            Json(serde_json::json!({ "ok": true, "mode": "in_place" })),
        )
            .into_response()),
        maco_harness::ResumeHitlOutcome::Stream { rx, .. } => {
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
            )
                .into_response())
        }
    }
}
