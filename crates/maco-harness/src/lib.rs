//! maco Agent 运行时：组装 adk `Runner`、ReAct 工具、HITL、Elicitation 与 SSE 事件流。

pub mod artifact_capture;
pub mod callbacks;
pub mod elicitation;
pub mod harness;
pub mod hitl;
pub mod mcp_pool;
pub mod run_stream;
pub mod model_factory;
pub mod orchestrator;
pub mod shell;
pub mod skills;
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
pub use skills::SkillLoader;
