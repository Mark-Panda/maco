//! MCP 服务配置与连接池重载路由。

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, patch, post},
};
use maco_core::MacoError;
use maco_db::{FILESYSTEM_MCP_NAME, McpServerRecord};
use serde::Deserialize;

use crate::AppState;
use crate::routes::ApiError;

/// MCP 管理路由，挂载于 `/api` 下。
pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/mcp/servers",
            get(list_mcp_servers).post(create_mcp_server),
        )
        .route(
            "/mcp/servers/{id}",
            patch(update_mcp_server).delete(delete_mcp_server),
        )
        .route("/mcp/reload", post(reload_mcp_pool))
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
    Ok(Json(state.repos.mcp_servers.list().await?))
}

/// `POST /mcp/servers` — 创建 MCP 服务配置并重载连接池。
async fn create_mcp_server(
    State(state): State<AppState>,
    Json(body): Json<McpServerUpsertBody>,
) -> Result<Json<McpServerRecord>, ApiError> {
    validate_mcp_body(&body)?;
    if body.name.trim() == FILESYSTEM_MCP_NAME {
        return Err(MacoError::config(
            "built-in filesystem MCP is managed by maco; cannot create manually",
        )
        .into());
    }
    let args = body.args.unwrap_or_else(|| "[]".into());
    let env = body.env.unwrap_or_else(|| "{}".into());
    let rec = state
        .repos
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
    if let Err(e) = state.reload_mcp_pool_guarded().await {
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
        .repos
        .mcp_servers
        .get(&id)
        .await?
        .ok_or_else(|| MacoError::not_found("mcp server"))?;
    if existing.name == FILESYSTEM_MCP_NAME {
        if body.name.trim() != FILESYSTEM_MCP_NAME {
            return Err(MacoError::config("built-in filesystem MCP cannot be renamed").into());
        }
        if body.transport.trim() != existing.transport {
            return Err(
                MacoError::config("built-in filesystem MCP transport cannot be changed").into(),
            );
        }
        if body.args.is_some() && body.args.as_ref() != Some(&existing.args) {
            return Err(MacoError::config(
                "filesystem MCP allowed roots are managed per-run by maco",
            )
            .into());
        }
        if body.url.is_some() {
            return Err(MacoError::config("built-in filesystem MCP does not use url").into());
        }
    }
    let args = body.args.unwrap_or(existing.args);
    let env = body.env.unwrap_or(existing.env);
    let enabled = body.enabled.unwrap_or(existing.enabled != 0);
    state
        .repos
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
    if let Err(e) = state.reload_mcp_pool_guarded().await {
        tracing::warn!("mcp reload after update: {e}");
    }
    state
        .repos
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
    let existing = state
        .repos
        .mcp_servers
        .get(&id)
        .await?
        .ok_or_else(|| MacoError::not_found("mcp server"))?;
    if existing.name == FILESYSTEM_MCP_NAME {
        return Err(MacoError::config("built-in filesystem MCP cannot be deleted").into());
    }
    if !state.repos.mcp_servers.delete(&id).await? {
        return Err(MacoError::not_found("mcp server").into());
    }
    if let Err(e) = state.reload_mcp_pool_guarded().await {
        tracing::warn!("mcp reload after delete: {e}");
    }
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /mcp/reload` — 从 DB 重载 MCP 连接池。
async fn reload_mcp_pool(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state.reload_mcp_pool_guarded().await?;
    let names = state.agent.mcp_pool.status_summary().await;
    let status = state.agent.mcp_pool.health_status().await;
    Ok(Json(serde_json::json!({
        "reloaded": true,
        "servers": names,
        "mcp_status": status,
    })))
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
