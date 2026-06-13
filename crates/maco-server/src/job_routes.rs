//! 后台任务管理路由。

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, patch, post},
};
use maco_core::MacoError;
use maco_db::JobRecord;
use serde::Deserialize;

use crate::AppState;
use crate::routes::ApiError;

/// 后台任务路由，挂载于 `/api` 下。
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/jobs", get(list_jobs).post(create_job))
        .route("/jobs/{id}", patch(update_job).delete(delete_job))
        .route("/jobs/{id}/run", post(run_job_now))
}

/// `POST /jobs` 请求体。
#[derive(Deserialize)]
struct CreateJobBody {
    /// 任务显示名称。
    name: String,
    /// 任务类型（如 `ping` / `log`）。
    job_type: String,
    /// 调度周期（`hourly` / `daily`，可选）。
    #[serde(default)]
    schedule: Option<String>,
    /// JSON 载荷字符串，默认 `{}`。
    #[serde(default)]
    payload: Option<String>,
    /// 首次执行时间（RFC3339，可选）。
    #[serde(default)]
    run_at: Option<String>,
}

/// `PATCH /jobs/{id}` 请求体。
#[derive(Deserialize)]
struct UpdateJobBody {
    /// 启用/禁用任务。
    #[serde(default)]
    enabled: Option<bool>,
}

/// `GET /jobs` — 列出所有后台任务。
async fn list_jobs(State(state): State<AppState>) -> Result<Json<Vec<JobRecord>>, ApiError> {
    Ok(Json(state.repos.jobs.list().await?))
}

/// `POST /jobs` — 创建后台任务。
async fn create_job(
    State(state): State<AppState>,
    Json(body): Json<CreateJobBody>,
) -> Result<Json<JobRecord>, ApiError> {
    if body.name.trim().is_empty() {
        return Err(MacoError::config("name required").into());
    }
    let payload = body.payload.unwrap_or_else(|| "{}".into());
    let next = body.run_at.as_deref();
    Ok(Json(
        state
            .repos
            .jobs
            .insert(
                body.name.trim(),
                body.job_type.trim(),
                body.schedule.as_deref(),
                &payload,
                next,
            )
            .await?,
    ))
}

/// `PATCH /jobs/{id}` — 更新任务（当前仅支持 enabled）。
async fn update_job(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<UpdateJobBody>,
) -> Result<StatusCode, ApiError> {
    if let Some(enabled) = body.enabled {
        state.repos.jobs.set_enabled(&id, enabled).await?;
    }
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /jobs/{id}` — 删除任务。
async fn delete_job(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    if !state.repos.jobs.delete(&id).await? {
        return Err(MacoError::not_found("job").into());
    }
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /jobs/{id}/run` — 立即执行一次任务（不等待调度）。
async fn run_job_now(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<JobRecord>, ApiError> {
    let job = state
        .repos
        .jobs
        .get(&id)
        .await?
        .ok_or_else(|| MacoError::not_found("job"))?;
    state
        .repos
        .jobs
        .update_run_result(&id, "running", None, None, None)
        .await?;
    let (status, result, err, next) = crate::worker::run_job_public(&job).await;
    state
        .repos
        .jobs
        .update_run_result(
            &id,
            &status,
            result.as_deref(),
            err.as_deref(),
            next.as_deref(),
        )
        .await?;
    state
        .repos
        .jobs
        .get(&id)
        .await?
        .ok_or_else(|| MacoError::not_found("job"))
        .map(Json)
        .map_err(Into::into)
}
