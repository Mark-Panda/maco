//! 模型调用用量统计：在 `after_model` 终态时写入 `maco_usage_stats`。

use std::sync::Arc;

use adk_core::{LlmResponse, UsageMetadata};
use maco_db::UsageRepo;
use maco_governance::{estimate_cost, ModelPricing};

/// 单次 Run 的用量写入上下文（绑定 session/run/model）。
#[derive(Clone)]
pub struct UsageContext {
    /// 用量写入仓库。
    pub repo: UsageRepo,
    /// 会话 ID。
    pub session_id: String,
    /// Run ID。
    pub run_id: String,
    /// 模型配置 ID。
    pub model_id: String,
    /// 模型显示名。
    pub model_name: String,
    /// 单价配置（用于估算费用）。
    pub pricing: ModelPricing,
}

impl UsageContext {
    /// 仅在流式响应终态且含 token 计数时落库，避免重复记账。
    pub async fn record_if_final(&self, response: &LlmResponse) {
        let Some(usage) = response.usage_metadata.as_ref() else {
            return;
        };
        if response.partial && !response.turn_complete {
            return;
        }
        if usage.prompt_token_count == 0 && usage.candidates_token_count == 0 {
            return;
        }
        let prompt = usage.prompt_token_count as i64;
        let completion = usage.candidates_token_count as i64;
        let cost = estimate_cost(&self.pricing, prompt, completion, usage.cost);
        if let Err(e) = self
            .repo
            .insert(
                Some(&self.session_id),
                Some(&self.run_id),
                Some(&self.model_id),
                &self.model_name,
                prompt,
                completion,
                cost,
            )
            .await
        {
            tracing::error!("usage stats write failed: {e}");
        }
    }
}

/// 从 adk 用量元数据提取 prompt/completion token 数。
pub fn usage_from_metadata(usage: &UsageMetadata) -> (i64, i64) {
    (
        usage.prompt_token_count as i64,
        usage.candidates_token_count as i64,
    )
}

/// 可在多回调间共享的用量上下文句柄。
pub type SharedUsageContext = Arc<UsageContext>;
