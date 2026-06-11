use sha2::{Digest, Sha256};

pub const SCOPE_ADMIN: &str = "admin";
pub const SCOPE_CHAT: &str = "chat";
pub const SCOPE_MEMORY: &str = "memory";
pub const SCOPE_EXPORT: &str = "export";

pub fn hash_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

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

pub fn auth_disabled() -> bool {
    std::env::var("MACO_AUTH_DISABLED")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(true)
}

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
