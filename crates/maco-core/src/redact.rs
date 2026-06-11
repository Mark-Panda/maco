//! 日志与 SSE 载荷脱敏：截断超大 JSON、掩码 API Key 等敏感片段。

use std::env;

const DEFAULT_MAX_BYTES: usize = 8192;

/// 超过 8KB 的 JSON 序列化结果截断，防止 callback 日志膨胀。
pub fn truncate_json(value: &serde_json::Value) -> serde_json::Value {
    let s = value.to_string();
    if s.len() <= DEFAULT_MAX_BYTES {
        return value.clone();
    }
    serde_json::Value::String(format!("{}… [truncated {} bytes]", &s[..DEFAULT_MAX_BYTES], s.len()))
}

/// 基础字符串脱敏；`MACO_LOG_REDACT=off` 时原样返回。
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

/// 先截断 JSON 再执行基础脱敏，供 callback 日志使用。
pub fn prepare_log_payload(value: &serde_json::Value) -> String {
    let truncated = truncate_json(value);
    basic_redact(&truncated.to_string())
}
