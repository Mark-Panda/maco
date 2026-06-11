use serde_json::{json, Value};

use crate::error::{MacoError, MacoResult};

const API_KEY_FIELD: &str = "api_key";

pub fn api_key_from_config(config: &str) -> Option<String> {
    let cfg: Value = serde_json::from_str(config).ok()?;
    cfg.get(API_KEY_FIELD)
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

pub fn has_stored_api_key(config: &str) -> bool {
    api_key_from_config(config).is_some()
}

pub fn redact_config_for_api(config: &str) -> String {
    let Ok(mut cfg) = serde_json::from_str::<Value>(config) else {
        return "{}".into();
    };
    if let Some(obj) = cfg.as_object_mut() {
        obj.remove(API_KEY_FIELD);
    }
    let redacted = obj_or_empty(&cfg);
    serde_json::to_string(&redacted).unwrap_or_else(|_| "{}".into())
}

fn obj_or_empty(v: &Value) -> Value {
    if v.is_object() {
        v.clone()
    } else {
        json!({})
    }
}

pub fn merge_api_key(config: &str, api_key: Option<&str>) -> MacoResult<String> {
    let mut cfg: Value = if config.trim().is_empty() {
        json!({})
    } else {
        serde_json::from_str(config)
            .map_err(|e| MacoError::config(format!("invalid model config json: {e}")))?
    };
    let obj = cfg
        .as_object_mut()
        .ok_or_else(|| MacoError::config("model config must be a JSON object"))?;
    match api_key {
        Some(key) if !key.trim().is_empty() => {
            obj.insert(API_KEY_FIELD.into(), json!(key.trim()));
        }
        Some(_) => {
            obj.remove(API_KEY_FIELD);
        }
        None => {}
    }
    serde_json::to_string(&cfg).map_err(|e| MacoError::config(e.to_string()))
}

pub fn api_key_preview(config: &str) -> Option<String> {
    let key = api_key_from_config(config)?;
    if key.len() <= 8 {
        return Some("••••".into());
    }
    Some(format!("{}…{}", &key[..4], &key[key.len() - 4..]))
}
