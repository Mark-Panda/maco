//! 模型配置路由。

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, patch},
};

use crate::AppState;
use crate::models_api::{ModelUpsertBody, ModelView, list_views, upsert_from_body};
use crate::routes::ApiError;

/// 模型配置路由，挂载于 `/api` 下。
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/models", get(list_models).post(create_model))
        .route("/models/{id}", patch(update_model).delete(delete_model))
}

/// `GET /models` — 列出模型配置（api_key 已脱敏）。
async fn list_models(State(state): State<AppState>) -> Result<Json<Vec<ModelView>>, ApiError> {
    Ok(Json(list_views(&state.repos.models).await?))
}

/// `POST /models` — 新建模型配置。
async fn create_model(
    State(state): State<AppState>,
    Json(body): Json<ModelUpsertBody>,
) -> Result<Json<ModelView>, ApiError> {
    Ok(Json(
        upsert_from_body(&state.repos.models, None, body).await?,
    ))
}

/// `PATCH /models/{id}` — 更新指定模型。
async fn update_model(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<ModelUpsertBody>,
) -> Result<Json<ModelView>, ApiError> {
    Ok(Json(
        upsert_from_body(&state.repos.models, Some(&id), body).await?,
    ))
}

/// `DELETE /models/{id}` — 删除模型配置。
async fn delete_model(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    state.repos.models.delete(&id).await?;
    Ok(StatusCode::NO_CONTENT)
}
