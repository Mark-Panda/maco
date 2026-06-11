//! LLM 用量费用估算（基于模型 config 中的单价配置）。

use maco_db::ModelRecord;

/// 模型单价配置（从 `config` JSON 解析）。
#[derive(Debug, Clone, Default)]
pub struct ModelPricing {
    /// 每 1K 输入 token 价格（美元）。
    pub price_per_1k_prompt: f64,
    /// 每 1K 输出 token 价格（美元）。
    pub price_per_1k_completion: f64,
}

pub fn pricing_from_model(model: &ModelRecord) -> ModelPricing {
    let Ok(cfg) = serde_json::from_str::<serde_json::Value>(&model.config) else {
        return ModelPricing::default();
    };
    ModelPricing {
        price_per_1k_prompt: cfg
            .get("price_per_1k_prompt")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0),
        price_per_1k_completion: cfg
            .get("price_per_1k_completion")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0),
    }
}

pub fn estimate_cost(
    pricing: &ModelPricing,
    prompt_tokens: i64,
    completion_tokens: i64,
    provider_cost: Option<f64>,
) -> Option<f64> {
    if let Some(c) = provider_cost {
        return Some(c);
    }
    if pricing.price_per_1k_prompt == 0.0 && pricing.price_per_1k_completion == 0.0 {
        return None;
    }
    let cost = (prompt_tokens as f64 / 1000.0) * pricing.price_per_1k_prompt
        + (completion_tokens as f64 / 1000.0) * pricing.price_per_1k_completion;
    Some(cost)
}
