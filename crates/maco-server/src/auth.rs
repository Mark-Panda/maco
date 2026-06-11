use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use maco_governance::{
    hash_token, has_scope, parse_scopes, required_scope_for_path, SCOPE_ADMIN,
};
use tracing::warn;

use crate::AppState;

#[derive(Clone, Debug)]
pub struct AuthContext {
    pub scopes: Vec<String>,
}

pub async fn auth_middleware(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Response {
    if state.auth_disabled {
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

    if path == "/api/auth/tokens" && method == "POST" {
        if let Ok(count) = state.api_tokens.count_enabled().await {
            if count == 0 {
                return next.run(req).await;
            }
        }
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
    let record = match state.api_tokens.find_by_hash(&hash).await {
        Ok(Some(r)) => r,
        Ok(None) => return unauthorized("invalid token"),
        Err(e) => {
            warn!("auth lookup failed: {e}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    if let Some(exp) = &record.expires_at {
        if let Ok(exp_dt) = chrono::DateTime::parse_from_rfc3339(exp) {
            if exp_dt < chrono::Utc::now() {
                return unauthorized("token expired");
            }
        }
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

    let _ = state.api_tokens.touch_last_used(&record.id).await;
    req.extensions_mut().insert(AuthContext { scopes });

    next.run(req).await
}

fn unauthorized(msg: &str) -> Response {
    (StatusCode::UNAUTHORIZED, msg.to_string()).into_response()
}

pub fn require_admin(ctx: Option<&AuthContext>) -> Result<(), StatusCode> {
    if let Some(ctx) = ctx {
        if has_scope(&ctx.scopes, SCOPE_ADMIN) || has_scope(&ctx.scopes, "*") {
            return Ok(());
        }
    }
    Err(StatusCode::FORBIDDEN)
}
