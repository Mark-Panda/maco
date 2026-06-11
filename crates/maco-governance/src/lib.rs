//! 治理层：鉴权 scope、HITL 策略、PII 脱敏、artifact 校验、用量估价。

pub mod artifact;
pub mod auth;
pub mod guardrail;
pub mod hitl;
pub mod usage;

pub use artifact::{
    allowed_mime, is_previewable_mime, mime_for_artifact, validate_artifact, MAX_ARTIFACT_BYTES,
};
pub use auth::{
    auth_disabled, generate_token, hash_token, has_scope, parse_scopes, required_scope_for_path,
    scopes_json, SCOPE_ADMIN, SCOPE_CHAT, SCOPE_EXPORT, SCOPE_MEMORY,
};
pub use hitl::{resolve_action, PolicyAction};
pub use usage::{estimate_cost, pricing_from_model, ModelPricing};
pub use guardrail::{
    pii_guardrail_enabled, prepare_log_payload, redact_sse_payload, redact_text,
};

/// 对任意文本做基础 PII 脱敏（委托 `guardrail::redact_text`）。
pub fn redact_basic(text: &str) -> String {
    redact_text(text)
}
