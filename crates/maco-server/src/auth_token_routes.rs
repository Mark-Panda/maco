//! API Token 管理路由。

use axum::{
    Extension, Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get},
};
use maco_core::MacoError;
use maco_governance::{SCOPE_ADMIN, generate_token, hash_token, scopes_json};
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::auth::{AuthContext, require_admin};
use crate::routes::ApiError;

/// API Token 路由，挂载于 `/api` 下。
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/auth/tokens", get(list_tokens).post(create_token))
        .route("/auth/tokens/{id}", delete(revoke_token))
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
    let count = state.repos.api_tokens.count_enabled().await?;
    if count > 0 {
        let auth = auth.ok_or_else(|| MacoError::config("unauthorized"))?;
        require_admin(Some(&auth.0)).map_err(|_| MacoError::config("admin scope required"))?;
    }
    let scopes = body
        .scopes
        .unwrap_or_else(|| vec![SCOPE_ADMIN.into(), "*".into()]);
    let raw = generate_token();
    let hash = hash_token(&raw);
    let record = state
        .repos
        .api_tokens
        .insert(
            &body.name,
            &hash,
            &scopes_json(&scopes),
            body.expires_at.as_deref(),
        )
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
    Ok(Json(state.repos.api_tokens.list().await?))
}

/// `DELETE /auth/tokens/{id}` — 吊销 API Token（需 admin scope）。
async fn revoke_token(
    State(state): State<AppState>,
    auth: Extension<AuthContext>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    require_admin(Some(&auth)).map_err(|_| MacoError::config("admin scope required"))?;
    if !state.repos.api_tokens.delete(&id).await? {
        return Err(MacoError::not_found("token").into());
    }
    Ok(StatusCode::NO_CONTENT)
}
