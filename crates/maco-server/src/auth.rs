//! Axum Bearer 鉴权中间件与请求扩展 `AuthContext`。

use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use maco_governance::{SCOPE_ADMIN, has_scope, hash_token, parse_scopes, required_scope_for_path};
use tracing::warn;

use crate::AppState;

/// 鉴权通过后注入请求的 scope 列表。
#[derive(Clone, Debug)]
pub struct AuthContext {
    /// 当前 Token 拥有的 scope 列表（含 `*` 表示全部）。
    pub scopes: Vec<String>,
}

/// 校验 `Authorization: Bearer`，并按路由检查 scope。
pub async fn auth_middleware(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Response {
    if state.runtime.auth_disabled {
        req.extensions_mut().insert(AuthContext {
            scopes: vec!["*".into()],
        });
        return next.run(req).await;
    }

    let path = req.uri().path().to_string();
    let method = req.method().as_str().to_string();

    if path == "/api/health" {
        return next.run(req).await;
    }

    if path == "/api/auth/tokens"
        && method == "POST"
        && let Ok(count) = state.repos.api_tokens.count_enabled().await
        && count == 0
    {
        return next.run(req).await;
    }

    let Some(auth_header) = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
    else {
        return unauthorized("missing Authorization header");
    };

    let token = auth_header
        .strip_prefix("Bearer ")
        .or_else(|| auth_header.strip_prefix("bearer "))
        .unwrap_or(auth_header);

    let hash = hash_token(token);
    let record = match state.repos.api_tokens.find_by_hash(&hash).await {
        Ok(Some(r)) => r,
        Ok(None) => return unauthorized("invalid token"),
        Err(e) => {
            warn!("auth lookup failed: {e}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    if let Some(exp) = &record.expires_at
        && let Ok(exp_dt) = chrono::DateTime::parse_from_rfc3339(exp)
        && exp_dt < chrono::Utc::now()
    {
        return unauthorized("token expired");
    }

    let scopes = parse_scopes(&record.scopes);
    if let Some(required) = required_scope_for_path(&path, &method) {
        if !has_scope(&scopes, required) {
            return StatusCode::FORBIDDEN.into_response();
        }
    } else if !has_scope(&scopes, SCOPE_ADMIN)
        && !has_scope(&scopes, "*")
        && !has_scope(&scopes, "chat")
    {
        // read-only API routes need any valid token
    }

    let _ = state.repos.api_tokens.touch_last_used(&record.id).await;
    req.extensions_mut().insert(AuthContext { scopes });

    next.run(req).await
}

fn unauthorized(msg: &str) -> Response {
    (StatusCode::UNAUTHORIZED, msg.to_string()).into_response()
}

pub fn require_admin(ctx: Option<&AuthContext>) -> Result<(), StatusCode> {
    if let Some(ctx) = ctx
        && (has_scope(&ctx.scopes, SCOPE_ADMIN) || has_scope(&ctx.scopes, "*"))
    {
        return Ok(());
    }
    Err(StatusCode::FORBIDDEN)
}
