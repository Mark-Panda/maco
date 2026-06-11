use std::sync::Arc;

use adk_core::{LlmResponse, UsageMetadata};
use maco_db::UsageRepo;
use maco_governance::{estimate_cost, ModelPricing};

#[derive(Clone)]
pub struct UsageContext {
    pub repo: UsageRepo,
    pub session_id: String,
    pub run_id: String,
    pub model_id: String,
    pub model_name: String,
    pub pricing: ModelPricing,
}

impl UsageContext {
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

pub fn usage_from_metadata(usage: &UsageMetadata) -> (i64, i64) {
    (
        usage.prompt_token_count as i64,
        usage.candidates_token_count as i64,
    )
}

pub type SharedUsageContext = Arc<UsageContext>;
