use std::sync::OnceLock;

use adk_guardrail::PiiRedactor;

static PII: OnceLock<PiiRedactor> = OnceLock::new();

fn pii_redactor() -> &'static PiiRedactor {
    PII.get_or_init(PiiRedactor::new)
}

pub fn pii_guardrail_enabled() -> bool {
    match std::env::var("MACO_GUARDRAIL_PII") {
        Ok(v) => !(v == "off" || v == "0" || v.eq_ignore_ascii_case("false")),
        Err(_) => true,
    }
}

/// Redact PII + sensitive token patterns for logs and SSE.
pub fn redact_text(text: &str) -> String {
    let out = if pii_guardrail_enabled() {
        pii_redactor().redact(text).0
    } else {
        text.to_string()
    };
    maco_core::basic_redact(&out)
}

pub fn prepare_log_payload(value: &serde_json::Value) -> String {
    redact_text(&maco_core::truncate_json(value).to_string())
}

pub fn redact_sse_payload(payload: &mut serde_json::Value) {
    if let Some(content) = payload.get_mut("content").and_then(|v| v.as_str()) {
        let redacted = redact_text(content);
        *payload = serde_json::json!({ "content": redacted });
    }
    if let Some(message) = payload.get_mut("message").and_then(|v| v.as_str()) {
        let redacted = redact_text(message);
        if let Some(obj) = payload.as_object_mut() {
            obj.insert("message".into(), serde_json::Value::String(redacted));
        }
    }
}
