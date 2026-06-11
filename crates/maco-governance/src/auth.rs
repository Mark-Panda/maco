//! Bearer Token 哈希、scope 解析与路由级权限映射。

use sha2::{Digest, Sha256};

/// 管理类 API（模型写、用量、jobs、token 管理）。
pub const SCOPE_ADMIN: &str = "admin";
/// 聊天 SSE 与中断。
pub const SCOPE_CHAT: &str = "chat";
/// Memory CRUD / 检索。
pub const SCOPE_MEMORY: &str = "memory";
/// 会话 Markdown 导出。
pub const SCOPE_EXPORT: &str = "export";

/// SHA-256 十六进制摘要，用于存库比对。
pub fn hash_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

/// 生成 `maco_` 前缀的明文 API Token（仅创建时返回一次）。
pub fn generate_token() -> String {
    format!("maco_{}", uuid::Uuid::new_v4().simple())
}

pub fn parse_scopes(raw: &str) -> Vec<String> {
    serde_json::from_str(raw).unwrap_or_else(|_| vec!["*".into()])
}

pub fn scopes_json(scopes: &[String]) -> String {
    serde_json::to_string(scopes).unwrap_or_else(|_| r#"["*"]"#.into())
}

pub fn has_scope(scopes: &[String], required: &str) -> bool {
    scopes.iter().any(|s| s == "*" || s == required)
}

/// 是否关闭鉴权（默认 `true`，本地开发友好）。
pub fn auth_disabled() -> bool {
    std::env::var("MACO_AUTH_DISABLED")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(true)
}

/// 按路径与方法返回所需 scope；`None` 表示仅需有效 Token 或公开。
pub fn required_scope_for_path(path: &str, method: &str) -> Option<&'static str> {
    if path.starts_with("/api/auth/tokens") {
        return Some(SCOPE_ADMIN);
    }
    if path.starts_with("/api/memory") {
        return Some(SCOPE_MEMORY);
    }
    if path.starts_with("/api/usage") || path.starts_with("/api/jobs") {
        return Some(SCOPE_ADMIN);
    }
    if path.starts_with("/api/models") && method != "GET" {
        return Some(SCOPE_ADMIN);
    }
    if path.starts_with("/api/chat") {
        return Some(SCOPE_CHAT);
    }
    if path.starts_with("/api/sessions/") && path.ends_with("/export") && method == "GET" {
        return Some(SCOPE_EXPORT);
    }
    None
}
