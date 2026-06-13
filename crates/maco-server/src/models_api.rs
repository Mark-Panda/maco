//! 模型 API 的 DTO 转换：内联 api_key 合并与列表脱敏。

use chrono::Utc;
use maco_core::{
    MacoError, MacoResult, api_key_preview, has_stored_api_key, merge_api_key,
    redact_config_for_api,
};
use maco_db::{ModelRecord, ModelRepo};
use maco_harness::validate_provider;
use serde::{Deserialize, Serialize};

/// 对外模型视图（不含明文 api_key，仅 preview）。
#[derive(Debug, Serialize)]
pub struct ModelView {
    /// 模型配置 ID。
    pub id: String,
    /// 显示名称。
    pub name: String,
    /// 提供商：`openai` / `anthropic` / `gemini` / `openrouter`。
    pub provider: String,
    /// 上游模型标识（如 `gpt-4o`、`claude-sonnet-4-20250514`）。
    pub model_id: String,
    /// 自定义 API Base URL（OpenAI 兼容或 Anthropic 代理）。
    pub base_url: Option<String>,
    /// 环境变量名，用于从进程环境读取 API Key 兜底。
    pub api_key_env: String,
    /// 是否为默认模型。
    pub is_default: bool,
    /// 是否启用。
    pub enabled: bool,
    /// JSON 扩展配置（api_key 已脱敏）。
    pub config: String,
    /// 是否已配置内联 api_key 或非空 api_key_env。
    pub has_api_key: bool,
    /// api_key 尾号预览（如 `...abcd`）。
    pub api_key_preview: Option<String>,
    /// 创建时间。
    pub created_at: String,
    /// 最后更新时间。
    pub updated_at: String,
}

/// `POST/PATCH /models` 请求体。
#[derive(Debug, Deserialize)]
pub struct ModelUpsertBody {
    /// 显示名称。
    pub name: String,
    /// 提供商：`openai` / `anthropic` / `gemini` / `openrouter`。
    pub provider: String,
    /// 上游模型标识。
    pub model_id: String,
    /// 自定义 API Base URL。
    #[serde(default)]
    pub base_url: Option<String>,
    /// 环境变量名（读取 API Key 兜底）。
    #[serde(default)]
    pub api_key_env: Option<String>,
    /// 内联 API Key；传空字符串表示清除已存储的 key。
    #[serde(default)]
    pub api_key: Option<String>,
    /// 是否设为默认模型。
    #[serde(default)]
    pub is_default: bool,
    /// 是否启用，默认 `true`。
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// 额外 JSON 配置（与 api_key 合并写入 config 列）。
    #[serde(default)]
    pub config: Option<String>,
}

fn default_enabled() -> bool {
    true
}

/// MiniMax 走 Anthropic 兼容协议，不能使用官方 `api.anthropic.com`。
fn validate_minimax_endpoint(
    provider: &str,
    model_id: &str,
    base_url: Option<&str>,
) -> MacoResult<()> {
    if provider != "anthropic" {
        return Ok(());
    }
    if !model_id.to_lowercase().contains("minimax") {
        return Ok(());
    }
    let base = base_url
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("https://api.anthropic.com");
    let base_lower = base.to_lowercase();
    if base_lower.contains("api.anthropic.com") || !base_lower.contains("minimax") {
        return Err(MacoError::config(
            "MiniMax 模型需将 base_url 设为 https://api.minimax.io/anthropic \
             （国内可用 https://api.minimaxi.com/anthropic），并使用 MiniMax API Key",
        ));
    }
    Ok(())
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
            has_api_key: has_stored_api_key(&rec.config) || (!rec.api_key_env.trim().is_empty()),
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
    validate_provider(&body.provider)?;
    if body.name.trim().is_empty() || body.model_id.trim().is_empty() {
        return Err(MacoError::config("name and model_id are required"));
    }

    let existing = if let Some(id) = id {
        repo.get(id).await?
    } else {
        None
    };

    let model_id = id.map(str::to_string).unwrap_or_else(ModelRepo::new_id);

    let base_config = body
        .config
        .as_deref()
        .or_else(|| existing.as_ref().map(|e| e.config.as_str()))
        .unwrap_or("{}");

    let api_key_input = body.api_key.as_deref();

    let merged_config = if api_key_input.is_some() {
        merge_api_key(base_config, api_key_input)?
    } else if let Some(ref ex) = existing {
        ex.config.clone()
    } else {
        merge_api_key(base_config, None)?
    };

    let upstream_model_id = body.model_id.trim().to_string();
    let base_url = match body.base_url {
        Some(url) => {
            let trimmed = url.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        None => existing.as_ref().and_then(|e| e.base_url.clone()),
    };
    validate_minimax_endpoint(&body.provider, &upstream_model_id, base_url.as_deref())?;

    let now = Utc::now().to_rfc3339();
    let rec = ModelRecord {
        id: model_id.clone(),
        name: body.name.trim().to_string(),
        provider: body.provider.clone(),
        model_id: upstream_model_id,
        base_url,
        api_key_env: body
            .api_key_env
            .unwrap_or_else(|| {
                existing
                    .as_ref()
                    .map(|e| e.api_key_env.clone())
                    .unwrap_or_default()
            })
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
