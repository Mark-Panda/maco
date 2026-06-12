//! ADK Runner 上下文压缩：跨轮次摘要、单轮 token 阈值、溢出截断。

use std::sync::Arc;

use adk_agent::LlmEventSummarizer;
use adk_core::{BaseEventsSummarizer, EventsCompactionConfig, IntraCompactionConfig, Llm};
use adk_runner::compaction::{CompactionConfig, TruncationCompaction};

/// 每 N 次用户 invocation 触发一次跨轮次 LLM 摘要。
pub const DEFAULT_COMPACTION_INTERVAL: u32 = 5;
/// 摘要窗口与上一轮重叠的事件数（保持连贯性）。
pub const DEFAULT_OVERLAP_SIZE: u32 = 2;
/// 单次 invocation 内估计 token 超过此值时触发摘要。
pub const DEFAULT_INTRA_TOKEN_THRESHOLD: u64 = 80_000;
/// 模型返回 token limit 错误时的截断预算（启发式 token 数）。
pub const DEFAULT_CONTEXT_BUDGET: usize = 96_000;
/// 溢出截断时保留的最近事件数（不含首条 system）。
pub const DEFAULT_TRUNCATION_PRESERVE_RECENT: usize = 12;

/// Runner 侧三类压缩配置（`MACO_COMPACTION=0` 可整体关闭）。
pub struct RunnerCompactionOptions {
    pub events: EventsCompactionConfig,
    pub intra: IntraCompactionConfig,
    pub intra_summarizer: Arc<dyn BaseEventsSummarizer>,
    pub overflow: CompactionConfig,
}

/// 是否启用 Runner 压缩（`MACO_COMPACTION=0` 关闭）。
pub fn compaction_enabled() -> bool {
    !matches!(
        std::env::var("MACO_COMPACTION").as_deref(),
        Ok("0") | Ok("false") | Ok("off")
    )
}

/// 基于当前会话 LLM 构建默认压缩配置。
pub fn runner_compaction_options(llm: Arc<dyn Llm>) -> RunnerCompactionOptions {
    let summarizer_for_events: Arc<dyn BaseEventsSummarizer> =
        Arc::new(LlmEventSummarizer::new(llm.clone()));
    let summarizer_for_intra: Arc<dyn BaseEventsSummarizer> =
        Arc::new(LlmEventSummarizer::new(llm));

    RunnerCompactionOptions {
        events: EventsCompactionConfig {
            compaction_interval: env_u32("MACO_COMPACTION_INTERVAL", DEFAULT_COMPACTION_INTERVAL),
            overlap_size: env_u32("MACO_COMPACTION_OVERLAP", DEFAULT_OVERLAP_SIZE),
            summarizer: summarizer_for_events,
        },
        intra: IntraCompactionConfig {
            token_threshold: env_u64(
                "MACO_INTRA_COMPACTION_TOKENS",
                DEFAULT_INTRA_TOKEN_THRESHOLD,
            ),
            overlap_event_count: 10,
            chars_per_token: 4,
        },
        intra_summarizer: summarizer_for_intra,
        overflow: CompactionConfig::new(
            Box::new(TruncationCompaction {
                preserve_recent: DEFAULT_TRUNCATION_PRESERVE_RECENT,
            }),
            env_usize("MACO_CONTEXT_BUDGET", DEFAULT_CONTEXT_BUDGET),
        ),
    }
}

fn env_u32(key: &str, default: u32) -> u32 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
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
    fn default_constants_are_sane() {
        assert!(DEFAULT_COMPACTION_INTERVAL >= 2);
        assert!(DEFAULT_INTRA_TOKEN_THRESHOLD >= 10_000);
        assert!(DEFAULT_CONTEXT_BUDGET >= DEFAULT_TRUNCATION_PRESERVE_RECENT);
    }
}
