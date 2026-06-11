use std::env;

const DEFAULT_MAX_BYTES: usize = 8192;

pub fn truncate_json(value: &serde_json::Value) -> serde_json::Value {
    let s = value.to_string();
    if s.len() <= DEFAULT_MAX_BYTES {
        return value.clone();
    }
    serde_json::Value::String(format!("{}… [truncated {} bytes]", &s[..DEFAULT_MAX_BYTES], s.len()))
}

pub fn basic_redact(text: &str) -> String {
    if env::var("MACO_LOG_REDACT").unwrap_or_else(|_| "basic".into()) == "off" {
        return text.to_string();
    }
    let mut out = text.to_string();
    for pattern in ["sk-ant-", "sk-proj-", "sk-", "Bearer ", "api_key", "x-api-key"] {
        if out.contains(pattern) {
            out = out.replace(pattern, "[REDACTED]");
        }
    }
    out
}

pub fn prepare_log_payload(value: &serde_json::Value) -> String {
    let truncated = truncate_json(value);
    basic_redact(&truncated.to_string())
}
