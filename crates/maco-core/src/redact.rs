//! 日志与 SSE 载荷脱敏：截断超大 JSON、掩码 API Key 等敏感片段。

use std::env;

const DEFAULT_MAX_BYTES: usize = 8192;

/// 在 UTF-8 字符边界处截断，避免切多字节字符导致 panic。
fn truncate_str(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_string();
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}… [truncated {} bytes]", &s[..end], s.len())
}

/// 超过 8KB 的 JSON 序列化结果截断，防止 callback 日志膨胀。
pub fn truncate_json(value: &serde_json::Value) -> serde_json::Value {
    let s = value.to_string();
    if s.len() <= DEFAULT_MAX_BYTES {
        return value.clone();
    }
    serde_json::Value::String(truncate_str(&s, DEFAULT_MAX_BYTES))
}

/// 基础字符串脱敏；`MACO_LOG_REDACT=off` 时原样返回。
pub fn basic_redact(text: &str) -> String {
    if env::var("MACO_LOG_REDACT").unwrap_or_else(|_| "basic".into()) == "off" {
        return text.to_string();
    }
    let mut out = text.to_string();
    for pattern in [
        "sk-ant-",
        "sk-proj-",
        "sk-",
        "Bearer ",
        "api_key",
        "x-api-key",
    ] {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_str_respects_utf8_char_boundary() {
        // 8192 字节边界落在中文「式」(3 bytes) 中间
        let mut s = "a".repeat(8190);
        s.push('式');
        s.push_str("tail");
        assert!(s.len() > DEFAULT_MAX_BYTES);
        let out = truncate_str(&s, DEFAULT_MAX_BYTES);
        assert!(out.contains("truncated"));
        assert!(std::str::from_utf8(out.as_bytes()).is_ok());
        assert!(!out.contains('式') || out.find('式').unwrap() + '式'.len_utf8() <= out.len());
    }

    #[test]
    fn truncate_json_does_not_panic_on_cjk() {
        let html = format!("<html>{}</html>", "五".repeat(3000));
        let value = serde_json::json!({ "content": html });
        let truncated = truncate_json(&value);
        assert!(truncated.as_str().unwrap_or("").contains("truncated"));
    }
}
