//! HITL 工具策略与 worktree 路径守卫路由。

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, patch, post},
};
use maco_core::MacoError;
use maco_db::{ToolPolicyRecord, WORKTREE_PATH_GUARD_KEY};
use serde::Deserialize;

use crate::AppState;
use crate::routes::ApiError;

/// 工具策略路由，挂载于 `/api` 下。
pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/tool-policies",
            get(list_tool_policies).post(create_tool_policy),
        )
        .route("/tool-policies/reload", post(reload_tool_policies))
        .route(
            "/tool-policies/worktree-guard",
            get(get_worktree_path_guard).patch(update_worktree_path_guard),
        )
        .route(
            "/tool-policies/{id}",
            patch(update_tool_policy).delete(delete_tool_policy),
        )
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

#[derive(Deserialize)]
struct WorktreePathGuardBody {
    enabled: bool,
}

async fn reload_harness_policies(state: &AppState) -> Result<(), MacoError> {
    let policies = state.repos.tool_policies.list_enabled().await?;
    state.agent.harness.set_tool_policies(policies).await;
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
) -> Result<Json<Vec<ToolPolicyRecord>>, ApiError> {
    Ok(Json(state.repos.tool_policies.list().await?))
}

/// `GET /tool-policies/worktree-guard` — worktree 主仓库路径拦截开关。
async fn get_worktree_path_guard(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(serde_json::json!({
        "enabled": state.agent.harness.worktree_path_guard_enabled().await,
    })))
}

/// `PATCH /tool-policies/worktree-guard` — 更新 worktree 路径拦截并热更新 Harness。
async fn update_worktree_path_guard(
    State(state): State<AppState>,
    Json(body): Json<WorktreePathGuardBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let value = if body.enabled { "true" } else { "false" };
    state
        .repos
        .settings
        .set(WORKTREE_PATH_GUARD_KEY, value)
        .await?;
    state
        .agent
        .harness
        .set_worktree_path_guard(body.enabled)
        .await;
    Ok(Json(serde_json::json!({ "enabled": body.enabled })))
}

/// `POST /tool-policies` — 新增策略并热更新 Harness。
async fn create_tool_policy(
    State(state): State<AppState>,
    Json(body): Json<ToolPolicyUpsertBody>,
) -> Result<Json<ToolPolicyRecord>, ApiError> {
    validate_policy_body(&body)?;
    let rec = state
        .repos
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
) -> Result<Json<ToolPolicyRecord>, ApiError> {
    validate_policy_body(&body)?;
    let existing = state
        .repos
        .tool_policies
        .get(&id)
        .await?
        .ok_or_else(|| MacoError::not_found("tool policy"))?;
    let enabled = body.enabled.unwrap_or(existing.enabled != 0);
    state
        .repos
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
        .repos
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
    if !state.repos.tool_policies.delete(&id).await? {
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
    let count = state.repos.tool_policies.list_enabled().await?.len();
    Ok(Json(
        serde_json::json!({ "reloaded": true, "enabled_count": count }),
    ))
}
