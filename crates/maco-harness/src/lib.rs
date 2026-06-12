//! maco Agent 运行时：组装 adk `Runner`、ReAct 工具、HITL、Elicitation 与 SSE 事件流。

pub mod adk_skills;
pub mod artifact_capture;
pub mod compaction;
pub mod callbacks;
pub mod elicitation;
pub mod harness;
pub mod hitl;
pub mod mcp_pool;
pub mod run_stream;
pub mod model_factory;
pub use model_factory::{validate_provider, SUPPORTED_PROVIDERS};
pub mod orchestrator;
pub mod shell;
pub mod skill_coordinator;
pub mod skill_install;
pub mod tool_concurrency;
pub mod usage;

pub use elicitation::{
    DynamicElicitationHandler, ElicitationBroker, ElicitationRespondBody, ElicitationRunContext,
    MacoElicitationHandler,
};
pub use harness::{MacoHarness, ResumeHitlOutcome};
pub use hitl::HitlBroker;
pub use run_stream::RunStreamRegistry;
pub use mcp_pool::McpPool;
pub use orchestrator::RunOrchestrator;
pub use adk_skills::{default_selection_policy, AdkSkillManager};
pub use compaction::{compaction_enabled, runner_compaction_options, RunnerCompactionOptions};
pub use tool_concurrency::{
    runner_run_config, tool_concurrency_config, tool_concurrency_enabled,
};
pub use skill_coordinator::{MacoToolRegistry, default_coordinator_config};
pub use skill_install::{delete_skill, install_skill_zip, SkillInstallResult, MAX_SKILL_ZIP_BYTES};
pub use adk_skill::{SkillDocument, SkillIndex};
