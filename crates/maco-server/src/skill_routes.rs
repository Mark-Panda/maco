//! Skill 管理路由。

use axum::{
    Json, Router,
    extract::{Multipart, Path, State},
    routing::get,
};
use maco_core::MacoError;
use maco_harness::{delete_skill, install_skill_zip};
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::routes::ApiError;
use crate::skills_sync::{self, SkillSummary, SkillUploadResponse, skill_to_summary};

/// Skill 管理路由，挂载于 `/api` 下。
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/skills", get(list_skills).post(upload_skill_zip))
        .route(
            "/skills/{name}",
            get(get_skill)
                .patch(patch_skill)
                .delete(delete_skill_handler),
        )
}

/// `GET /skills` — 扫描本地 Skill 目录并返回摘要列表（含启用状态）。
async fn list_skills(State(state): State<AppState>) -> Result<Json<Vec<SkillSummary>>, ApiError> {
    Ok(Json(
        skills_sync::sync_skills(&state.repos.skills, state.agent.adk_skills.as_ref(), None)
            .await?,
    ))
}

/// `POST /skills` — multipart 上传 Skill zip（字段 `file`，可选 `overwrite=true`）。
async fn upload_skill_zip(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<SkillUploadResponse>, ApiError> {
    let mut filename = "skill.zip".to_string();
    let mut bytes: Vec<u8> = Vec::new();
    let mut overwrite = false;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| MacoError::config(e.to_string()))?
    {
        let name = field.name().unwrap_or_default();
        match name {
            "file" => {
                filename = field
                    .file_name()
                    .map(str::to_string)
                    .unwrap_or_else(|| "skill.zip".into());
                bytes = field
                    .bytes()
                    .await
                    .map_err(|e| MacoError::config(e.to_string()))?
                    .to_vec();
            }
            "overwrite" => {
                let v = field
                    .text()
                    .await
                    .map_err(|e| MacoError::config(e.to_string()))?;
                overwrite = matches!(v.trim(), "1" | "true" | "yes" | "on");
            }
            _ => {}
        }
    }

    if bytes.is_empty() {
        return Err(MacoError::config("file field is required").into());
    }

    let result = install_skill_zip(&bytes, &filename, overwrite)?;
    state
        .repos
        .skills
        .upsert_from_scan(
            &result.skill.name,
            &result.skill.description,
            &result.skill.path.display().to_string(),
        )
        .await?;
    state
        .repos
        .skills
        .set_enabled(&result.skill.name, true)
        .await?;
    skills_sync::sync_skills(&state.repos.skills, state.agent.adk_skills.as_ref(), None).await?;
    Ok(Json(SkillUploadResponse {
        name: result.skill.name.clone(),
        description: result.skill.description.clone(),
        file_path: result.skill.path.display().to_string(),
        extracted_files: result.extracted_files,
        skill: skill_to_summary(&result.skill, true),
    }))
}

/// `PATCH /skills/{name}` — 启用或禁用 Skill。
#[derive(Deserialize)]
struct PatchSkillBody {
    enabled: bool,
}

async fn patch_skill(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(body): Json<PatchSkillBody>,
) -> Result<Json<SkillSummary>, ApiError> {
    let _ = skills_sync::sync_skills(&state.repos.skills, state.agent.adk_skills.as_ref(), None)
        .await?;
    let skill = state
        .agent
        .adk_skills
        .find_by_name(&name)
        .ok_or_else(|| MacoError::not_found("skill"))?;
    state
        .repos
        .skills
        .upsert_from_scan(
            &skill.name,
            &skill.description,
            &skill.path.display().to_string(),
        )
        .await?;
    state.repos.skills.set_enabled(&name, body.enabled).await?;
    state.agent.adk_skills.set_enabled(&name, body.enabled);
    Ok(Json(skill_to_summary(&skill, body.enabled)))
}

/// `DELETE /skills/{name}` — 删除已安装的 Skill。
async fn delete_skill_handler(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    delete_skill(&name)?;
    let _ = state.repos.skills.delete_by_name(&name).await;
    state.agent.adk_skills.remove(&name);
    Ok(Json(serde_json::json!({ "deleted": true, "name": name })))
}

/// Skill 详情（含 Markdown 正文）。
#[derive(Serialize)]
struct SkillDetail {
    /// Skill 名称。
    name: String,
    /// 描述。
    description: String,
    /// 源文件路径。
    file_path: String,
    /// SKILL.md 正文。
    content: String,
    /// 是否启用。
    enabled: bool,
}

/// `GET /skills/{name}` — 获取单个 Skill 的完整内容。
async fn get_skill(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<SkillDetail>, ApiError> {
    skills_sync::reload_disabled(&state.repos.skills, state.agent.adk_skills.as_ref()).await?;
    let skill = state
        .agent
        .adk_skills
        .find_by_name(&name)
        .ok_or_else(|| MacoError::not_found("skill"))?;
    Ok(Json(SkillDetail {
        name: skill.name.clone(),
        description: skill.description.clone(),
        file_path: skill.path.display().to_string(),
        content: skill.body.clone(),
        enabled: state.agent.adk_skills.is_enabled(&name),
    }))
}
