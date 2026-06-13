//! maco Agent 运行时：组装 adk `Runner`、ReAct 工具、HITL、Elicitation 与 SSE 事件流。

pub mod adk_skills;
pub mod artifact_capture;
pub mod callbacks;
pub mod compaction;
pub mod elicitation;
pub mod filesystem_mcp;
pub mod force_unary_llm;
pub mod harness;
pub mod hitl;
pub mod mcp_pool;
pub mod model_activity;
pub mod model_factory;
pub mod run_stream;
pub mod session_context;
pub use model_factory::{
    DEFAULT_MAX_TOKENS, SUPPORTED_PROVIDERS, max_tokens_for_model, validate_provider,
};
pub mod orchestrator;
pub mod shell;
pub mod skill_coordinator;
pub mod skill_install;
pub mod sub_agent;
pub mod tool_concurrency;
pub mod usage;

pub use adk_skill::{SkillDocument, SkillIndex};
pub use adk_skills::{AdkSkillManager, default_selection_policy};
pub use compaction::{RunnerCompactionOptions, compaction_enabled, runner_compaction_options};
pub use elicitation::{
    DynamicElicitationHandler, ElicitationBroker, ElicitationRespondBody, ElicitationRunContext,
    MacoElicitationHandler,
};
pub use filesystem_mcp::FilesystemMcpCoordinator;
pub use harness::{MacoHarness, ResumeHitlOutcome};
pub use hitl::HitlBroker;
pub use mcp_pool::McpPool;
pub use orchestrator::RunOrchestrator;
pub use run_stream::RunStreamRegistry;
pub use skill_coordinator::{MacoToolRegistry, default_coordinator_config};
pub use skill_install::{MAX_SKILL_ZIP_BYTES, SkillInstallResult, delete_skill, install_skill_zip};
pub use tool_concurrency::{runner_run_config, tool_concurrency_config, tool_concurrency_enabled};
