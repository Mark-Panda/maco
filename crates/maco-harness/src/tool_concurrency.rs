//! ADK `RunConfig::tool_concurrency`：限制并行工具调用，bash 默认串行。

use std::collections::HashMap;

use adk_core::{BackpressurePolicy, RunConfig, ToolConcurrencyConfig};

/// 全局并行工具调用上限（不含 per-tool 更严限制）。
pub const DEFAULT_MAX_TOOL_CONCURRENCY: usize = 6;
/// `bash` 工具默认并发（1 = 同会话串行执行 shell）。
pub const DEFAULT_BASH_CONCURRENCY: usize = 1;

/// 是否启用工具并发限制（`MACO_TOOL_CONCURRENCY=0` 关闭，恢复 ADK 默认无限）。
pub fn tool_concurrency_enabled() -> bool {
    !matches!(
        std::env::var("MACO_TOOL_CONCURRENCY").as_deref(),
        Ok("0") | Ok("false") | Ok("off")
    )
}

/// 构建 maco 默认 `ToolConcurrencyConfig`。
pub fn tool_concurrency_config() -> ToolConcurrencyConfig {
    let max_concurrency = env_usize("MACO_TOOL_CONCURRENCY_MAX", DEFAULT_MAX_TOOL_CONCURRENCY);
    let bash_limit = env_usize("MACO_BASH_CONCURRENCY", DEFAULT_BASH_CONCURRENCY);

    let backpressure = match std::env::var("MACO_TOOL_BACKPRESSURE").as_deref() {
        Ok("fail") => BackpressurePolicy::Fail,
        _ => BackpressurePolicy::Queue,
    };

    let mut per_tool = HashMap::new();
    if bash_limit > 0 {
        per_tool.insert("bash".to_string(), bash_limit);
    }

    ToolConcurrencyConfig {
        max_concurrency: Some(max_concurrency),
        per_tool,
        backpressure,
    }
}

/// Runner `RunConfig`（当前仅含 tool concurrency，后续可扩展）。
pub fn runner_run_config() -> RunConfig {
    RunConfig::builder()
        .tool_concurrency(tool_concurrency_config())
        .build()
}

fn env_usize(key: &str, default: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_sets_global_and_bash_limits() {
        let cfg = tool_concurrency_config();
        assert_eq!(cfg.max_concurrency, Some(DEFAULT_MAX_TOOL_CONCURRENCY));
        assert_eq!(cfg.per_tool.get("bash"), Some(&DEFAULT_BASH_CONCURRENCY));
        assert_eq!(cfg.backpressure, BackpressurePolicy::Queue);
    }
}
