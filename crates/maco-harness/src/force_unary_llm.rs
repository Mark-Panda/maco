//! 对易在 SSE 流上失败的 Anthropic 兼容端点，强制走非流式 HTTP（仍返回单条 `LlmResponse` 流）。

use std::sync::Arc;

use adk_core::{AdkError, Llm, LlmRequest, LlmResponseStream, SchemaAdapter};
use async_trait::async_trait;
use maco_db::ModelRecord;

/// Anthropic 兼容代理（MiniMax 等）在长流式响应上偶发 `error decoding response body`。
pub fn should_force_unary_http(model: &ModelRecord) -> bool {
    if std::env::var("MACO_FORCE_UNARY_LLM")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
    {
        return true;
    }
    if model.provider != "anthropic" {
        return false;
    }
    let base = model
        .base_url
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("https://api.anthropic.com");
    let lower = base.to_lowercase();
    !lower.contains("api.anthropic.com")
}

/// 将 `generate_content(..., stream=true)` 转为底层 `stream=false`。
pub struct ForceUnaryLlm {
    inner: Arc<dyn Llm>,
}

impl ForceUnaryLlm {
    pub fn wrap(inner: Arc<dyn Llm>) -> Arc<dyn Llm> {
        Arc::new(Self { inner })
    }
}

#[async_trait]
impl Llm for ForceUnaryLlm {
    fn name(&self) -> &str {
        self.inner.name()
    }

    async fn generate_content(
        &self,
        req: LlmRequest,
        _stream: bool,
    ) -> Result<LlmResponseStream, AdkError> {
        self.inner.generate_content(req, false).await
    }

    fn schema_adapter(&self) -> &dyn SchemaAdapter {
        self.inner.schema_adapter()
    }

    fn uses_interactions_api(&self) -> bool {
        self.inner.uses_interactions_api()
    }
}
