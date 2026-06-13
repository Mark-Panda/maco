//! 根据 `maco_models` 记录构造 adk LLM 客户端。

use std::env;
use std::sync::{Arc, Mutex};

use adk_rust::prelude::*;
use maco_core::{api_key_from_config, MacoError, MacoResult};
use maco_db::ModelRecord;
use serde_json::Value;
use tracing::info;

use crate::force_unary_llm::{should_force_unary_http, ForceUnaryLlm};

/// ADK 默认仅 4096，工具调用 JSON 易被截断；maco 统一使用较大默认值。
pub const DEFAULT_MAX_TOKENS: u32 = 32_768;

/// LLM HTTP 请求超时（秒），可通过 `MACO_LLM_TIMEOUT_SECS` 覆盖（最小 30）。
pub fn llm_http_timeout_secs() -> u64 {
    std::env::var("MACO_LLM_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .filter(|&secs| secs >= 30)
        .unwrap_or(600)
}

pub fn max_tokens_for_model(model: &ModelRecord) -> u32 {
    let cfg: Value = serde_json::from_str(&model.config).unwrap_or(Value::Null);
    cfg.get("max_tokens")
        .and_then(|v| v.as_u64())
        .map(|n| n.min(524_288) as u32)
        .unwrap_or(DEFAULT_MAX_TOKENS)
}

/// adk-model 1.0.0 的 `AnthropicClient::new` 未把 `AnthropicConfig.base_url` 传给底层 HTTP 客户端，
/// 底层只读 `ANTHROPIC_BASE_URL` 环境变量。构建时短暂注入并加锁，避免并发 Run 互相覆盖。
static ANTHROPIC_CLIENT_BUILD_LOCK: Mutex<()> = Mutex::new(());

fn is_minimax_model(model: &ModelRecord) -> bool {
    model.model_id.to_lowercase().contains("minimax")
}

/// 构建 Anthropic 客户端，确保自定义 `base_url` 真正进入 HTTP 层。
fn build_anthropic_client(cfg: AnthropicConfig) -> MacoResult<AnthropicClient> {
    let _guard = ANTHROPIC_CLIENT_BUILD_LOCK
        .lock()
        .map_err(|e| MacoError::run(format!("anthropic client build lock poisoned: {e}")))?;

    let prev_base = env::var("ANTHROPIC_BASE_URL").ok();
    if let Some(base) = cfg.base_url.as_ref().filter(|b| !b.trim().is_empty()) {
        // Rust 2024: env mutation is unsafe when other threads may read the environment.
        unsafe {
            env::set_var("ANTHROPIC_BASE_URL", base);
        }
        info!(
            target: "maco::llm_dispatch",
            anthropic_http_base = %base,
            "inject ANTHROPIC_BASE_URL for ADK Anthropic HTTP client"
        );
    }

    let client = AnthropicClient::new(cfg).map_err(|e| MacoError::Adk(e.to_string()));

    unsafe {
        match prev_base {
            Some(prev) => env::set_var("ANTHROPIC_BASE_URL", prev),
            None => env::remove_var("ANTHROPIC_BASE_URL"),
        }
    }

    client
}

fn finalize_llm(llm: Arc<dyn Llm>, model: &ModelRecord) -> Arc<dyn Llm> {
    if should_force_unary_http(model) {
        ForceUnaryLlm::wrap(llm)
    } else {
        llm
    }
}

/// 支持的模型提供商。
pub const SUPPORTED_PROVIDERS: &[&str] = &["openai", "anthropic", "gemini", "openrouter"];

enum ApiKeySource {
    Inline,
    Env(String),
}

impl ApiKeySource {
    fn label(&self) -> String {
        match self {
            Self::Inline => "config.api_key".into(),
            Self::Env(name) => format!("env:{}", name),
        }
    }
}

/// 解析本次请求将使用的 base_url（含各 provider 的 ADK 默认值）。
pub fn effective_base_url(model: &ModelRecord) -> String {
    if let Some(base) = model.base_url.as_ref().filter(|b| !b.trim().is_empty()) {
        return base.clone();
    }
    match model.provider.as_str() {
        "openai" => "https://api.openai.com/v1".into(),
        "anthropic" => "https://api.anthropic.com".into(),
        "openrouter" => "https://openrouter.ai/api/v1".into(),
        "gemini" => "(gemini default)".into(),
        other => format!("({other} default)"),
    }
}

/// 打印即将用于上游请求的模型参数（本地排查用，含完整 api_key）。
fn log_llm_dispatch(
    model: &ModelRecord,
    api_key: &str,
    key_source: &ApiKeySource,
    session_id: Option<&str>,
    run_id: Option<&str>,
) {
    let base_url = effective_base_url(model);
    let max_tokens = max_tokens_for_model(model);
    info!(
        target: "maco::llm_dispatch",
        session_id = session_id.unwrap_or(""),
        run_id = run_id.unwrap_or(""),
        model_config_id = %model.id,
        model_name = %model.name,
        provider = %model.provider,
        upstream_model_id = %model.model_id,
        base_url = %base_url,
        max_tokens = max_tokens,
        force_unary_http = should_force_unary_http(model),
        llm_timeout_secs = llm_http_timeout_secs(),
        api_key_source = %key_source.label(),
        api_key = %api_key,
        "LLM dispatch — upstream request parameters"
    );
}

/// 校验 provider 是否在支持列表中。
pub fn validate_provider(provider: &str) -> MacoResult<()> {
    if SUPPORTED_PROVIDERS.contains(&provider) {
        Ok(())
    } else {
        Err(MacoError::config(format!(
            "unsupported provider: {provider} (supported: {})",
            SUPPORTED_PROVIDERS.join(", ")
        )))
    }
}

/// 优先使用模型 `config.api_key`，否则回退到 `api_key_env` 环境变量。
fn resolve_api_key(model: &ModelRecord) -> MacoResult<(String, ApiKeySource)> {
    if let Some(key) = api_key_from_config(&model.config) {
        return Ok((key, ApiKeySource::Inline));
    }
    if !model.api_key_env.trim().is_empty() {
        let name = model.api_key_env.clone();
        return env::var(&name)
            .map(|key| (key, ApiKeySource::Env(name)))
            .map_err(|_| {
                MacoError::config(format!(
                    "missing env {} — set API key in model settings or .env",
                    model.api_key_env
                ))
            });
    }
    Err(MacoError::config(format!(
        "model '{}' has no API key — configure in Settings",
        model.name
    )))
}

/// MiniMax 未配置 base_url 时 ADK 会默认 `api.anthropic.com`，须提前拦截。
fn validate_minimax_anthropic_endpoint(model: &ModelRecord) -> MacoResult<()> {
    if model.provider != "anthropic" {
        return Ok(());
    }
    if !model.model_id.to_lowercase().contains("minimax") {
        return Ok(());
    }
    let base = model
        .base_url
        .as_deref()
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

/// 按 provider 构建 `Arc<dyn Llm>`，支持自定义 `base_url`（OpenAI / Anthropic / OpenRouter）。
pub fn build_llm(model: &ModelRecord) -> MacoResult<Arc<dyn Llm>> {
    build_llm_for_run(model, None, None)
}

/// 带会话/Run 上下文构建 LLM，日志中会带上 `session_id` / `run_id`。
pub fn build_llm_for_run(
    model: &ModelRecord,
    session_id: Option<&str>,
    run_id: Option<&str>,
) -> MacoResult<Arc<dyn Llm>> {
    validate_provider(&model.provider)?;
    validate_minimax_anthropic_endpoint(model)?;
    let (api_key, key_source) = resolve_api_key(model)?;
    log_llm_dispatch(model, &api_key, &key_source, session_id, run_id);
    match model.provider.as_str() {
        "openai" => {
            let client = if let Some(base) = model.base_url.as_ref().filter(|b| !b.trim().is_empty()) {
                OpenAIClient::new(OpenAIConfig::compatible(api_key, base, &model.model_id))
            } else {
                OpenAIClient::new(OpenAIConfig::new(api_key, &model.model_id))
            }
            .map_err(|e| MacoError::Adk(e.to_string()))?;
            Ok(finalize_llm(Arc::new(client) as Arc<dyn Llm>, model))
        }
        "anthropic" => {
            let mut cfg = AnthropicConfig::new(api_key, &model.model_id)
                .with_max_tokens(max_tokens_for_model(model));
            if let Some(base) = model.base_url.as_ref().filter(|b| !b.trim().is_empty()) {
                cfg = cfg.with_base_url(base);
            }
            if is_minimax_model(model) {
                cfg = cfg.with_prompt_caching(false);
            }
            let client = build_anthropic_client(cfg)?;
            Ok(finalize_llm(Arc::new(client) as Arc<dyn Llm>, model))
        }
        "gemini" => {
            let client = GeminiModel::new(api_key, &model.model_id)
                .map_err(|e| MacoError::Adk(e.to_string()))?;
            Ok(finalize_llm(Arc::new(client) as Arc<dyn Llm>, model))
        }
        "openrouter" => {
            let mut cfg = OpenRouterConfig::new(api_key, &model.model_id);
            if let Some(base) = model.base_url.as_ref().filter(|b| !b.trim().is_empty()) {
                cfg = cfg.with_base_url(base);
            }
            let client = OpenRouterClient::new(cfg).map_err(|e| MacoError::Adk(e.to_string()))?;
            Ok(finalize_llm(Arc::new(client) as Arc<dyn Llm>, model))
        }
        other => Err(MacoError::config(format!("unsupported provider: {other}"))),
    }
}

#[cfg(test)]
mod force_unary_tests {
    use super::*;
    use crate::force_unary_llm::should_force_unary_http;

    #[test]
    fn force_unary_for_custom_anthropic_base_url() {
        let minimax = maco_db::ModelRecord {
            id: "1".into(),
            name: "m".into(),
            provider: "anthropic".into(),
            model_id: "MiniMax-M3".into(),
            base_url: Some("https://api.minimaxi.com/anthropic".into()),
            api_key_env: "".into(),
            is_default: 0,
            enabled: 1,
            config: "{}".into(),
            created_at: "".into(),
            updated_at: "".into(),
        };
        assert!(should_force_unary_http(&minimax));

        let claude = maco_db::ModelRecord {
            base_url: Some("https://api.anthropic.com".into()),
            ..minimax
        };
        assert!(!should_force_unary_http(&claude));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_provider_accepts_known() {
        for p in SUPPORTED_PROVIDERS {
            validate_provider(p).expect("known provider");
        }
    }

    #[test]
    fn validate_provider_rejects_unknown() {
        assert!(validate_provider("cohere").is_err());
    }

    #[test]
    fn minimax_rejects_official_anthropic_base_url() {
        let bad = maco_db::ModelRecord {
            id: "1".into(),
            name: "MiniMax".into(),
            provider: "anthropic".into(),
            model_id: "MiniMax-M3".into(),
            base_url: Some("https://api.anthropic.com".into()),
            api_key_env: "MINIMAX_API_KEY".into(),
            is_default: 0,
            enabled: 1,
            config: "{}".into(),
            created_at: "".into(),
            updated_at: "".into(),
        };
        assert!(validate_minimax_anthropic_endpoint(&bad).is_err());
        let good = maco_db::ModelRecord {
            base_url: Some("https://api.minimax.io/anthropic".into()),
            ..bad
        };
        assert!(validate_minimax_anthropic_endpoint(&good).is_ok());
    }

    #[test]
    fn max_tokens_uses_unified_default_and_config_override() {
        let base = maco_db::ModelRecord {
            id: "1".into(),
            name: "test".into(),
            provider: "anthropic".into(),
            model_id: "claude-sonnet-4-6".into(),
            base_url: None,
            api_key_env: "".into(),
            is_default: 0,
            enabled: 1,
            config: "{}".into(),
            created_at: "".into(),
            updated_at: "".into(),
        };
        assert_eq!(max_tokens_for_model(&base), DEFAULT_MAX_TOKENS);

        let minimax = maco_db::ModelRecord {
            model_id: "MiniMax-M3".into(),
            ..base.clone()
        };
        assert_eq!(max_tokens_for_model(&minimax), DEFAULT_MAX_TOKENS);

        let custom = maco_db::ModelRecord {
            config: r#"{"max_tokens":8192}"#.into(),
            ..base
        };
        assert_eq!(max_tokens_for_model(&custom), 8192);
    }

    #[test]
    fn llm_http_timeout_secs_resolves_env_and_default() {
        unsafe {
            std::env::remove_var("MACO_LLM_TIMEOUT_SECS");
            assert_eq!(llm_http_timeout_secs(), 600);
            std::env::set_var("MACO_LLM_TIMEOUT_SECS", "900");
            assert_eq!(llm_http_timeout_secs(), 900);
            std::env::remove_var("MACO_LLM_TIMEOUT_SECS");
        }
    }
}
