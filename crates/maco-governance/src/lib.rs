//! 治理层：鉴权 scope、HITL 策略、PII 脱敏、artifact 校验、用量估价。

pub mod artifact;
pub mod auth;
pub mod guardrail;
pub mod hitl;
pub mod usage;

pub use artifact::{
    MAX_ARTIFACT_BYTES, allowed_mime, is_previewable_mime, mime_for_artifact, validate_artifact,
};
pub use auth::{
    SCOPE_ADMIN, SCOPE_CHAT, SCOPE_EXPORT, SCOPE_MEMORY, auth_disabled, generate_token, has_scope,
    hash_token, parse_scopes, required_scope_for_path, scopes_json,
};
pub use guardrail::{pii_guardrail_enabled, prepare_log_payload, redact_sse_payload, redact_text};
pub use hitl::{PolicyAction, resolve_action, resolve_action_with_mode};
pub use usage::{ModelPricing, estimate_cost, pricing_from_model};

/// 对任意文本做基础 PII 脱敏（委托 `guardrail::redact_text`）。
pub fn redact_basic(text: &str) -> String {
    redact_text(text)
}
