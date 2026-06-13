//! 用量统计路由。

use axum::{Json, Router, extract::State, routing::get};
use maco_core::MacoError;
use serde::Deserialize;

use crate::AppState;
use crate::routes::ApiError;

/// 用量统计路由，挂载于 `/api` 下。
pub fn router() -> Router<AppState> {
    Router::new().route("/usage/summary", get(usage_summary))
}

/// `GET /usage/summary` 的 query 参数。
#[derive(Deserialize)]
struct UsageSummaryQuery {
    /// 统计起始时间（RFC3339，可选）。
    from: Option<String>,
    /// 统计结束时间（RFC3339，可选）。
    to: Option<String>,
    /// 聚合维度：`model` / `day` / `session`，默认 `model`。
    #[serde(default = "default_group_by")]
    group_by: String,
}

fn default_group_by() -> String {
    "model".into()
}

/// `GET /usage/summary` — 按维度汇总 token 用量与估算费用。
async fn usage_summary(
    State(state): State<AppState>,
    axum::extract::Query(q): axum::extract::Query<UsageSummaryQuery>,
) -> Result<Json<Vec<maco_db::UsageSummaryItem>>, ApiError> {
    let group_by = match q.group_by.as_str() {
        "model" | "day" | "session" => q.group_by.as_str(),
        _ => return Err(MacoError::config("group_by must be model, day, or session").into()),
    };
    Ok(Json(
        state
            .repos
            .usage
            .summary(q.from.as_deref(), q.to.as_deref(), group_by)
            .await?,
    ))
}
