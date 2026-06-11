use chrono::Utc;
use maco_core::{
    api_key_preview, has_stored_api_key, merge_api_key, redact_config_for_api, MacoError, MacoResult,
};
use maco_db::{ModelRecord, ModelRepo};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub struct ModelView {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub model_id: String,
    pub base_url: Option<String>,
    pub api_key_env: String,
    pub is_default: bool,
    pub enabled: bool,
    pub config: String,
    pub has_api_key: bool,
    pub api_key_preview: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct ModelUpsertBody {
    pub name: String,
    pub provider: String,
    pub model_id: String,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub api_key_env: Option<String>,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub is_default: bool,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub config: Option<String>,
}

fn default_enabled() -> bool {
    true
}

impl ModelView {
    pub fn from_record(rec: &ModelRecord) -> Self {
        Self {
            id: rec.id.clone(),
            name: rec.name.clone(),
            provider: rec.provider.clone(),
            model_id: rec.model_id.clone(),
            base_url: rec.base_url.clone(),
            api_key_env: rec.api_key_env.clone(),
            is_default: rec.is_default == 1,
            enabled: rec.enabled == 1,
            config: redact_config_for_api(&rec.config),
            has_api_key: has_stored_api_key(&rec.config)
                || (!rec.api_key_env.trim().is_empty()),
            api_key_preview: api_key_preview(&rec.config),
            created_at: rec.created_at.clone(),
            updated_at: rec.updated_at.clone(),
        }
    }
}

pub async fn list_views(repo: &ModelRepo) -> MacoResult<Vec<ModelView>> {
    Ok(repo
        .list()
        .await?
        .iter()
        .map(ModelView::from_record)
        .collect())
}

pub async fn upsert_from_body(
    repo: &ModelRepo,
    id: Option<&str>,
    body: ModelUpsertBody,
) -> MacoResult<ModelView> {
    if !matches!(body.provider.as_str(), "openai" | "anthropic") {
        return Err(MacoError::config("provider must be openai or anthropic"));
    }
    if body.name.trim().is_empty() || body.model_id.trim().is_empty() {
        return Err(MacoError::config("name and model_id are required"));
    }

    let existing = if let Some(id) = id {
        repo.get(id).await?
    } else {
        None
    };

    let model_id = id
        .map(str::to_string)
        .unwrap_or_else(ModelRepo::new_id);

    let base_config = body
        .config
        .as_deref()
        .or_else(|| existing.as_ref().map(|e| e.config.as_str()))
        .unwrap_or("{}");

    let api_key_input = match body.api_key.as_deref() {
        Some("") => Some(""), // clear stored key
        Some(key) => Some(key),
        None => None,         // keep existing
    };

    let merged_config = if api_key_input.is_some() {
        merge_api_key(base_config, api_key_input)?
    } else if let Some(ref ex) = existing {
        ex.config.clone()
    } else {
        merge_api_key(base_config, None)?
    };

    let now = Utc::now().to_rfc3339();
    let rec = ModelRecord {
        id: model_id.clone(),
        name: body.name.trim().to_string(),
        provider: body.provider.clone(),
        model_id: body.model_id.trim().to_string(),
        base_url: body
            .base_url
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
        api_key_env: body
            .api_key_env
            .unwrap_or_else(|| existing.as_ref().map(|e| e.api_key_env.clone()).unwrap_or_default())
            .trim()
            .to_string(),
        is_default: if body.is_default { 1 } else { 0 },
        enabled: if body.enabled { 1 } else { 0 },
        config: merged_config,
        created_at: existing
            .as_ref()
            .map(|e| e.created_at.clone())
            .unwrap_or_else(|| now.clone()),
        updated_at: now,
    };

    repo.upsert(&rec).await?;
    Ok(ModelView::from_record(&rec))
}
