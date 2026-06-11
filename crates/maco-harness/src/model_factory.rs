use std::env;
use std::sync::Arc;

use adk_rust::prelude::*;
use maco_core::{api_key_from_config, MacoError, MacoResult};
use maco_db::ModelRecord;

fn resolve_api_key(model: &ModelRecord) -> MacoResult<String> {
    if let Some(key) = api_key_from_config(&model.config) {
        return Ok(key);
    }
    if !model.api_key_env.trim().is_empty() {
        return env::var(&model.api_key_env).map_err(|_| {
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

pub fn build_llm(model: &ModelRecord) -> MacoResult<Arc<dyn Llm>> {
    let api_key = resolve_api_key(model)?;
    match model.provider.as_str() {
        "openai" => {
            let client = if let Some(base) = &model.base_url {
                OpenAIClient::new(OpenAIConfig::compatible(api_key, base, &model.model_id))
            } else {
                OpenAIClient::new(OpenAIConfig::new(api_key, &model.model_id))
            }
            .map_err(|e| MacoError::Adk(e.to_string()))?;
            Ok(Arc::new(client) as Arc<dyn Llm>)
        }
        "anthropic" => {
            let mut cfg = AnthropicConfig::new(api_key, &model.model_id);
            if let Some(base) = &model.base_url {
                cfg = cfg.with_base_url(base);
            }
            let client = AnthropicClient::new(cfg).map_err(|e| MacoError::Adk(e.to_string()))?;
            Ok(Arc::new(client) as Arc<dyn Llm>)
        }
        other => Err(MacoError::config(format!("unsupported provider: {other}"))),
    }
}
