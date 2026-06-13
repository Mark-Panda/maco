//! ReAct 子 Agent：`spawn_sub_agent` 工具与 `LlmAgent` 嵌套执行。

use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use adk_agent::LlmAgentBuilder;
use adk_core::{
    AdkError, Content, Event, InvocationContext as InvocationContextTrait, Part,
    Result as AdkResult, SharedState, Tool, ToolContext, Toolset,
};
use adk_runner::{InvocationContext, MutableSession};
use adk_session::{GetRequest, SessionService};
use adk_tool::LoadArtifactsTool;
use async_trait::async_trait;
use futures::StreamExt;
use maco_core::{MacoError, MacoResult, SessionWorkspace, SseEnvelope, APP_NAME, USER_ID};
use maco_db::{ReactRepo, SubAgentRunRepo};
use maco_storage::adk_artifacts_enabled;
use maco_telemetry::MacoCallbackLogger;
use serde_json::{json, Value};
use tokio::sync::{mpsc, Mutex};
use crate::callbacks::{after_tool, agent_guardrails, before_tool_with_hitl};
use crate::hitl::HitlGate;
use crate::orchestrator::RunOrchestrator;
use crate::run_stream::RunStreamRegistry;
use crate::usage::UsageContext;

const DEFAULT_MAX_SPAWNS: usize = 20;
const DEFAULT_MAX_ITERATIONS: u32 = 50;
const DEFAULT_TIMEOUT_SECS: u64 = 600;

/// 规整 `task_key` 为 agent 名安全片段。
pub fn sanitize_task_key(raw: &str) -> String {
    let s: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let trimmed = s.trim_matches('-');
    let capped: String = trimmed.chars().take(48).collect();
    if capped.is_empty() {
        "task".into()
    } else {
        capped
    }
}

/// 工作区 / worktree 说明片段（供主 Agent 与子 Agent instruction 复用）。
pub fn workspace_instruction(scratch_dir: &Path, workspace: Option<&SessionWorkspace>) -> String {
    let scratch = scratch_dir.display();
    let mut out = format!(
        "Scratch directory (temp only): `{scratch}`\n\
         - Put temporary/scratch artifacts here — not user project source files.\n",
    );
    if let Some(ws) = workspace {
        if ws.uses_worktree {
            let branch = ws.worktree_branch.as_deref().unwrap_or("(unknown)");
            out.push_str(&format!(
                "\nGit worktree workspace (MUST edit here): `{wt}`\n\
                 Worktree branch: `{branch}`\n\
                 Repository root (do NOT edit directly): `{repo}`\n\
                 - All code changes MUST stay inside the worktree workspace directory.\n\
                 - bash starts in the worktree; relative paths resolve there.\n\
                 - Never modify files in the main repository checkout.\n",
                wt = ws.workspace_root.display(),
                repo = ws.repo_root.display(),
            ));
        } else {
            out.push_str(&format!(
                "\nProject root (bound for this session): `{path}`\n\
                 - When editing the user's codebase, use paths under this directory.\n\
                 - bash starts in this directory; relative paths resolve here.\n",
                path = ws.workspace_root.display(),
            ));
        }
    } else {
        out.push_str("\nNo project root is bound — use absolute paths the user provides.\n");
    }
    out
}

fn max_spawns_per_run() -> usize {
    env::var("MACO_SUB_AGENT_MAX_SPAWNS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_MAX_SPAWNS)
}

fn max_sub_iterations() -> u32 {
    env::var("MACO_SUB_AGENT_MAX_ITERATIONS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_MAX_ITERATIONS as u32)
}

fn sub_agent_timeout() -> Duration {
    let secs = env::var("MACO_SUB_AGENT_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_TIMEOUT_SECS);
    Duration::from_secs(secs)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ToolsProfile {
    Coding,
    Readonly,
    Full,
}

impl ToolsProfile {
    fn parse(raw: Option<&str>) -> Self {
        match raw.map(str::trim).filter(|s| !s.is_empty()) {
            Some("readonly") => Self::Readonly,
            Some("full") => Self::Full,
            _ => Self::Coding,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Coding => "coding",
            Self::Readonly => "readonly",
            Self::Full => "full",
        }
    }
}

struct ActiveSubSpawn {
    task_key: String,
    record_id: String,
    cancelled: Arc<AtomicBool>,
}

/// 主 Run 与子 Agent 共享的可变 session 槽与结构化状态（E3/E4）。
#[derive(Clone)]
pub struct SubAgentParentBridge {
    mutable_session: Arc<Mutex<Option<Arc<MutableSession>>>>,
    shared_state: Arc<SharedState>,
}

impl SubAgentParentBridge {
    pub fn new() -> Self {
        Self {
            mutable_session: Arc::new(Mutex::new(None)),
            shared_state: Arc::new(SharedState::new()),
        }
    }

    pub fn shared_state(&self) -> Arc<SharedState> {
        Arc::clone(&self.shared_state)
    }

    /// spawn 前从 session service 加载最新事件，供子 Agent 继承父级上下文。
    pub async fn parent_mutable_session_for_spawn(
        &self,
        session_service: &Arc<dyn SessionService>,
        session_id: &str,
    ) -> MacoResult<Arc<MutableSession>> {
        let session_box = session_service
            .get(GetRequest {
                app_name: APP_NAME.to_string(),
                user_id: USER_ID.to_string(),
                session_id: session_id.to_string(),
                num_recent_events: None,
                after: None,
            })
            .await
            .map_err(|e| MacoError::Adk(e.to_string()))?;
        let ms = Arc::new(MutableSession::new(Arc::from(session_box)));
        *self.mutable_session.lock().await = Some(Arc::clone(&ms));
        Ok(ms)
    }

    pub async fn apply_sub_agent_event(&self, event: &Event) {
        if let Some(ms) = self.mutable_session.lock().await.as_ref() {
            if !event.actions.state_delta.is_empty() {
                ms.apply_state_delta(&event.actions.state_delta);
            }
            ms.append_event(event.clone());
        }
    }
}

fn shared_state_prefix(task_key: &str) -> String {
    format!("subagent:{}", task_key)
}

async fn write_sub_agent_shared_state(
    shared: &SharedState,
    task_key: &str,
    status: &str,
    summary: Option<&str>,
    artifacts: &[String],
    error: Option<&str>,
) {
    let prefix = shared_state_prefix(task_key);
    let _ = shared
        .set_shared(format!("{}:status", prefix), json!(status))
        .await;
    if let Some(s) = summary {
        let _ = shared
            .set_shared(format!("{}:result", prefix), json!(s))
            .await;
    }
    let _ = shared
        .set_shared(format!("{}:artifacts", prefix), json!(artifacts))
        .await;
    if let Some(e) = error {
        let _ = shared
            .set_shared(format!("{}:error", prefix), json!(e))
            .await;
    }
}

/// 按 parent_run_id 登记活跃子 Agent spawn，支持取消与级联中断。
#[derive(Clone, Default)]
pub struct SubAgentCoordinator {
    active: Arc<Mutex<HashMap<String, ActiveSubSpawn>>>,
}

impl SubAgentCoordinator {
    pub fn new() -> Self {
        Self {
            active: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn is_active(&self, parent_run_id: &str) -> bool {
        self.active.lock().await.contains_key(parent_run_id)
    }

    pub async fn try_register(
        &self,
        parent_run_id: &str,
        task_key: &str,
        record_id: &str,
    ) -> MacoResult<Arc<AtomicBool>> {
        let mut map = self.active.lock().await;
        if map.contains_key(parent_run_id) {
            return Err(MacoError::conflict(
                "another sub-agent is already running in this run",
            ));
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        map.insert(
            parent_run_id.to_string(),
            ActiveSubSpawn {
                task_key: task_key.to_string(),
                record_id: record_id.to_string(),
                cancelled: Arc::clone(&cancelled),
            },
        );
        Ok(cancelled)
    }

    pub async fn clear(&self, parent_run_id: &str) {
        self.active.lock().await.remove(parent_run_id);
    }

    /// 取消指定 task_key 的活跃 spawn；返回 `(task_key, record_id)`。
    pub async fn cancel(
        &self,
        parent_run_id: &str,
        task_key: &str,
    ) -> Option<(String, String)> {
        let map = self.active.lock().await;
        let entry = map.get(parent_run_id);
        if entry.map(|e| e.task_key == task_key).unwrap_or(false) {
            let spawn = map.get(parent_run_id).unwrap();
            spawn.cancelled.store(true, Ordering::Relaxed);
            return Some((spawn.task_key.clone(), spawn.record_id.clone()));
        }
        None
    }

    /// 主 Run 中断时级联取消；返回 `(task_key, record_id)`。
    pub async fn cancel_all_for_run(&self, parent_run_id: &str) -> Option<(String, String)> {
        let map = self.active.lock().await;
        if let Some(entry) = map.get(parent_run_id) {
            entry.cancelled.store(true, Ordering::Relaxed);
            return Some((entry.task_key.clone(), entry.record_id.clone()));
        }
        None
    }
}

/// 单次 Run 内 spawn 子 Agent 所需的共享上下文。
pub struct SubAgentRunContext {
    inner: Arc<SubAgentRunContextInner>,
}

struct SubAgentRunContextInner {
    session_id: String,
    run_id: String,
    llm: Arc<dyn adk_core::Llm>,
    max_output_tokens: i32,
    scratch_dir: PathBuf,
    workspace: Option<SessionWorkspace>,
    worktree_path_guard: bool,
    bash_tool: Arc<dyn Tool>,
    mcp_toolsets: Vec<Arc<dyn Toolset>>,
    react: ReactRepo,
    session_service: Arc<dyn SessionService>,
    memory: Arc<dyn adk_core::Memory>,
    logger: Arc<MacoCallbackLogger>,
    hitl: Arc<HitlGate>,
    usage: Option<Arc<UsageContext>>,
    streams: RunStreamRegistry,
    sse_tx: mpsc::Sender<SseEnvelope>,
    orchestrator: RunOrchestrator,
    spawn_count: AtomicUsize,
    sub_agent_runs: SubAgentRunRepo,
    model_id: String,
    coordinator: SubAgentCoordinator,
    parent_bridge: SubAgentParentBridge,
}

impl SubAgentRunContext {
    pub fn new(
        session_id: String,
        run_id: String,
        llm: Arc<dyn adk_core::Llm>,
        max_output_tokens: i32,
        scratch_dir: PathBuf,
        workspace: Option<SessionWorkspace>,
        worktree_path_guard: bool,
        bash_tool: Arc<dyn Tool>,
        mcp_toolsets: Vec<Arc<dyn Toolset>>,
        react: ReactRepo,
        session_service: Arc<dyn SessionService>,
        memory: Arc<dyn adk_core::Memory>,
        logger: Arc<MacoCallbackLogger>,
        hitl: Arc<HitlGate>,
        usage: Option<Arc<UsageContext>>,
        streams: RunStreamRegistry,
        sse_tx: mpsc::Sender<SseEnvelope>,
        orchestrator: RunOrchestrator,
        sub_agent_runs: SubAgentRunRepo,
        model_id: String,
        coordinator: SubAgentCoordinator,
        parent_bridge: SubAgentParentBridge,
    ) -> Self {
        Self {
            inner: Arc::new(SubAgentRunContextInner {
                session_id,
                run_id,
                llm,
                max_output_tokens,
                scratch_dir,
                workspace,
                worktree_path_guard,
                bash_tool,
                mcp_toolsets,
                react,
                session_service,
                memory,
                logger,
                hitl,
                usage,
                streams,
                sse_tx,
                orchestrator,
                spawn_count: AtomicUsize::new(0),
                sub_agent_runs,
                model_id,
                coordinator,
                parent_bridge,
            }),
        }
    }

    pub fn spawn_tool(&self) -> Arc<dyn Tool> {
        Arc::new(SpawnSubAgentTool {
            ctx: self.clone(),
        })
    }
}

impl Clone for SubAgentRunContext {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

struct SpawnSubAgentTool {
    ctx: SubAgentRunContext,
}

#[async_trait]
impl Tool for SpawnSubAgentTool {
    fn name(&self) -> &str {
        "spawn_sub_agent"
    }

    fn description(&self) -> &str {
        "Spawn an adk LlmAgent sub-worker for a single ReAct todo step. \
         Provide task_key and a detailed instruction. Returns summary when done."
    }

    fn parameters_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "task_key": {
                    "type": "string",
                    "description": "Stable todo identifier aligned with upsert_todo"
                },
                "instruction": {
                    "type": "string",
                    "description": "Sub-task goal, constraints, and acceptance criteria"
                },
                "title": {
                    "type": "string",
                    "description": "Optional todo title override"
                },
                "tools_profile": {
                    "type": "string",
                    "enum": ["coding", "readonly", "full"],
                    "description": "Tool set for the sub-agent (default: coding)"
                }
            },
            "required": ["task_key", "instruction"]
        }))
    }

    async fn execute(&self, ctx: Arc<dyn ToolContext>, args: Value) -> AdkResult<Value> {
        let task_key_raw = args
            .get("task_key")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        let instruction = args
            .get("instruction")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        if task_key_raw.is_empty() || instruction.is_empty() {
            return Err(AdkError::new(
                adk_core::ErrorComponent::Tool,
                adk_core::ErrorCategory::InvalidInput,
                "spawn_sub_agent",
                "task_key and instruction are required",
            ));
        }

        let task_key = sanitize_task_key(task_key_raw);
        let title = args.get("title").and_then(|v| v.as_str());
        let profile = ToolsProfile::parse(args.get("tools_profile").and_then(|v| v.as_str()));

        let inner = &self.ctx.inner;
        if inner.spawn_count.load(Ordering::Relaxed) >= max_spawns_per_run() {
            return Err(AdkError::new(
                adk_core::ErrorComponent::Tool,
                adk_core::ErrorCategory::RateLimited,
                "spawn_sub_agent",
                format!("spawn limit reached (max {} per run)", max_spawns_per_run()),
            ));
        }

        if inner.coordinator.is_active(&inner.run_id).await {
            return Err(AdkError::new(
                adk_core::ErrorComponent::Tool,
                adk_core::ErrorCategory::Unavailable,
                "spawn_sub_agent",
                "another sub-agent is already running in this run",
            ));
        }

        let result = self
            .run_spawn(ctx.session_id(), &task_key, instruction, title, profile)
            .await;

        match result {
            Ok(v) => Ok(v),
            Err(e) => Err(AdkError::new(
                adk_core::ErrorComponent::Tool,
                adk_core::ErrorCategory::Internal,
                "spawn_sub_agent",
                e.to_string(),
            )),
        }
    }
}

impl SpawnSubAgentTool {
    async fn run_spawn(
        &self,
        session_id: &str,
        task_key: &str,
        instruction: &str,
        title: Option<&str>,
        profile: ToolsProfile,
    ) -> MacoResult<Value> {
        let inner = &self.ctx.inner;
        if session_id != inner.session_id {
            return Err(MacoError::conflict("spawn_sub_agent session mismatch"));
        }

        let sort_order = 0i64;
        let todo_title = title
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(task_key);
        inner
            .react
            .upsert_todo(session_id, task_key, todo_title, sort_order, Some("in_progress"))
            .await
            .map_err(|e| MacoError::database(e.to_string()))?;

        let worker_name = format!("worker-{}", task_key);
        let record = inner
            .sub_agent_runs
            .start(
                session_id,
                &inner.run_id,
                task_key,
                &worker_name,
                profile.as_str(),
                instruction,
                Some(&inner.model_id),
            )
            .await?;

        let cancelled_flag = match inner
            .coordinator
            .try_register(&inner.run_id, task_key, &record.id)
            .await
        {
            Ok(flag) => flag,
            Err(e) => {
                inner
                    .sub_agent_runs
                    .finish(
                        &record.id,
                        "failed",
                        None,
                        Some("another sub-agent is already running in this run"),
                        None,
                    )
                    .await?;
                return Err(e);
            }
        };

        write_sub_agent_shared_state(
            &inner.parent_bridge.shared_state,
            task_key,
            "running",
            None,
            &[],
            None,
        )
        .await;

        let sub_instruction =
            build_sub_instruction(instruction, &inner.scratch_dir, inner.workspace.as_ref());

        let worker = build_worker_agent(inner, &worker_name, &sub_instruction, profile)?;

        let mutable_session = inner
            .parent_bridge
            .parent_mutable_session_for_spawn(&inner.session_service, session_id)
            .await?;

        let invocation_id = format!("{}-sub-{}", inner.run_id, task_key);
        let user_content = Content {
            role: "user".into(),
            parts: vec![Part::Text {
                text: instruction.to_string(),
            }],
        };

        let inv_ctx = InvocationContext::with_mutable_session(
            invocation_id,
            worker.clone(),
            USER_ID.to_string(),
            APP_NAME.to_string(),
            session_id.to_string(),
            user_content,
            mutable_session,
        )
        .map_err(|e| MacoError::Adk(e.to_string()))?
        .with_branch(format!("sub/{}", task_key))
        .with_memory(inner.memory.clone())
        .with_shared_state(inner.parent_bridge.shared_state());

        let run_result = tokio::time::timeout(
            sub_agent_timeout(),
            run_worker_stream(
                worker,
                inv_ctx,
                inner,
                task_key,
                &worker_name,
                cancelled_flag,
            ),
        )
        .await;

        inner.coordinator.clear(&inner.run_id).await;
        inner.spawn_count.fetch_add(1, Ordering::Relaxed);

        let outcome = match run_result {
            Err(_) => WorkerOutcome {
                status: "timeout".into(),
                summary: None,
                artifacts: Vec::new(),
                error: Some(format!(
                    "sub-agent timed out after {}s",
                    sub_agent_timeout().as_secs()
                )),
            },
            Ok(inner_result) => match inner_result {
                Ok(outcome) => outcome,
                Err(e) => WorkerOutcome {
                    status: "failed".into(),
                    summary: None,
                    artifacts: Vec::new(),
                    error: Some(e.to_string()),
                },
            },
        };

        inner
            .sub_agent_runs
            .finish(
                &record.id,
                &outcome.status,
                outcome.summary.as_deref(),
                outcome.error.as_deref(),
                None,
            )
            .await?;

        write_sub_agent_shared_state(
            &inner.parent_bridge.shared_state,
            task_key,
            &outcome.status,
            outcome.summary.as_deref(),
            &outcome.artifacts,
            outcome.error.as_deref(),
        )
        .await;

        if outcome.status == "completed" {
            inner
                .react
                .upsert_todo(session_id, task_key, todo_title, sort_order, Some("completed"))
                .await
                .map_err(|e| MacoError::database(e.to_string()))?;
        }

        Ok(json!({
            "task_key": task_key,
            "status": outcome.status,
            "summary": outcome.summary,
            "artifacts": outcome.artifacts,
            "worker_agent": worker_name,
            "error": outcome.error,
            "sub_agent_run_id": record.id,
        }))
    }
}

struct WorkerOutcome {
    status: String,
    summary: Option<String>,
    artifacts: Vec<String>,
    error: Option<String>,
}

async fn run_worker_stream(
    worker: Arc<dyn adk_core::Agent>,
    inv_ctx: InvocationContext,
    inner: &SubAgentRunContextInner,
    task_key: &str,
    worker_agent: &str,
    cancelled_flag: Arc<AtomicBool>,
) -> MacoResult<WorkerOutcome> {
    let mut stream = worker
        .run(Arc::new(inv_ctx) as Arc<dyn InvocationContextTrait>)
        .await
        .map_err(|e| MacoError::Adk(e.to_string()))?;

    let mut last_summary = String::new();
    let artifacts: Vec<String> = Vec::new();
    let mut failed = false;
    let mut fail_msg: Option<String> = None;
    let mut last_progress_emit = Instant::now() - Duration::from_secs(3600);
    let throttle = Duration::from_millis(sub_agent_sse_throttle_ms());

    while let Some(item) = stream.next().await {
        if cancelled_flag.load(Ordering::Relaxed) {
            return Ok(WorkerOutcome {
                status: "cancelled".into(),
                summary: None,
                artifacts,
                error: Some("cancelled".into()),
            });
        }

        match item {
            Ok(event) => {
                if let Some(text) = extract_event_text(&event) {
                    if event.llm_response.partial {
                        if text.len() >= 80 || last_progress_emit.elapsed() >= throttle {
                            publish_sub_agent_progress(
                                inner,
                                task_key,
                                worker_agent,
                                &text,
                                "text",
                            )
                            .await;
                            last_progress_emit = Instant::now();
                        }
                    } else if !text.is_empty() {
                        last_summary = text;
                    }
                }
                if event.llm_response.partial {
                    continue;
                }
                if let Some(tool_ev) = extract_tool_event(&event) {
                    publish_sub_tool_call(inner, task_key, &tool_ev).await;
                }
                if let Err(e) = inner
                    .session_service
                    .append_event(&inner.session_id, event.clone())
                    .await
                {
                    tracing::warn!("sub-agent append_event: {e}");
                }
                inner.parent_bridge.apply_sub_agent_event(&event).await;
            }
            Err(e) => {
                failed = true;
                fail_msg = Some(e.to_string());
                break;
            }
        }
    }

    if failed {
        return Ok(WorkerOutcome {
            status: "failed".into(),
            summary: None,
            artifacts,
            error: fail_msg,
        });
    }

    let summary = if last_summary.is_empty() {
        "已完成，无文本输出".to_string()
    } else {
        last_summary
    };

    Ok(WorkerOutcome {
        status: "completed".into(),
        summary: Some(summary),
        artifacts,
        error: None,
    })
}

fn build_sub_instruction(
    task: &str,
    scratch_dir: &Path,
    workspace: Option<&SessionWorkspace>,
) -> String {
    let ws = workspace_instruction(scratch_dir, workspace);
    format!(
        "You are a maco sub-agent executing a single ReAct todo step.\n\
         Do NOT call update_plan, upsert_todo, or spawn_sub_agent.\n\
         Parent run context is shared via session history — use it for background, \
         but complete only the assigned task below.\n\
         Complete only the assigned task and end with a concise summary of what you did.\n\n\
         ## Task\n{task}\n\n\
         {ws}",
    )
}

fn build_worker_agent(
    inner: &SubAgentRunContextInner,
    worker_name: &str,
    instruction: &str,
    profile: ToolsProfile,
) -> MacoResult<Arc<dyn adk_core::Agent>> {
    let logger = Arc::clone(&inner.logger);
    let hitl = Arc::clone(&inner.hitl);
    let workspace = inner.workspace.clone();
    let worktree_guard = inner.worktree_path_guard;

    let mut builder = LlmAgentBuilder::new(worker_name)
        .description("maco ReAct sub-agent worker")
        .instruction(instruction)
        .model(Arc::clone(&inner.llm))
        .max_output_tokens(inner.max_output_tokens)
        .max_iterations(max_sub_iterations())
        .disallow_transfer_to_parent(true)
        .disallow_transfer_to_peers(true)
        .input_guardrails(agent_guardrails())
        .output_guardrails(agent_guardrails())
        .before_callback(crate::callbacks::before_agent(Arc::clone(&logger)))
        .after_callback(crate::callbacks::after_agent(Arc::clone(&logger)))
        .before_model_callback(crate::callbacks::before_model(Arc::clone(&logger)))
        .after_model_callback(
            crate::callbacks::after_model(Arc::clone(&logger), inner.usage.clone()),
        )
        .before_tool_callback(before_tool_with_hitl(
            Arc::clone(&logger),
            hitl,
            workspace,
            worktree_guard,
        ))
        .after_tool_callback(after_tool(Arc::clone(&logger)));

    match profile {
        ToolsProfile::Readonly => {
            for ts in &inner.mcp_toolsets {
                builder = builder.toolset(Arc::clone(ts));
            }
        }
        ToolsProfile::Coding | ToolsProfile::Full => {
            builder = builder.tool(Arc::clone(&inner.bash_tool));
            for ts in &inner.mcp_toolsets {
                builder = builder.toolset(Arc::clone(ts));
            }
            if adk_artifacts_enabled() {
                builder = builder.tool(Arc::new(LoadArtifactsTool::new()));
            }
        }
    }

    let agent = builder
        .build()
        .map_err(|e| MacoError::Adk(e.to_string()))?;
    Ok(Arc::new(agent))
}

const DEFAULT_SSE_THROTTLE_MS: u64 = 200;

fn sub_agent_sse_throttle_ms() -> u64 {
    env::var("MACO_SUB_AGENT_SSE_THROTTLE_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_SSE_THROTTLE_MS)
}

async fn publish_sub_agent_progress(
    inner: &SubAgentRunContextInner,
    task_key: &str,
    worker_agent: &str,
    content: &str,
    phase: &str,
) {
    let seq = inner.orchestrator.next_seq(&inner.run_id).await.unwrap_or(0);
    let env = SseEnvelope {
        event_type: "sub_agent_progress".into(),
        run_id: inner.run_id.clone(),
        seq,
        payload: json!({
            "task_key": task_key,
            "worker_agent": worker_agent,
            "phase": phase,
            "content": content,
        }),
    };
    let _ = inner.sse_tx.send(env.clone()).await;
    inner.streams.publish(&inner.session_id, env).await;
}

async fn publish_sub_tool_call(
    inner: &SubAgentRunContextInner,
    task_key: &str,
    tool_ev: &Value,
) {
    let seq = inner.orchestrator.next_seq(&inner.run_id).await.unwrap_or(0);
    let payload = json!({
        "name": tool_ev.get("name"),
        "args": tool_ev.get("args"),
        "call_id": tool_ev.get("call_id"),
        "sub_agent": true,
        "task_key": task_key,
        "parent_tool": "spawn_sub_agent",
    });
    let env = SseEnvelope {
        event_type: "tool_call".into(),
        run_id: inner.run_id.clone(),
        seq,
        payload,
    };
    let _ = inner.sse_tx.send(env.clone()).await;
    inner.streams.publish(&inner.session_id, env).await;
}

/// 向活跃 Run SSE 推送子 Agent 取消事件（供 harness cancel / interrupt 调用）。
pub async fn emit_sub_agent_cancelled(
    streams: &RunStreamRegistry,
    orchestrator: &RunOrchestrator,
    session_id: &str,
    run_id: &str,
    task_key: &str,
    reason: &str,
) {
    let seq = orchestrator.next_seq(run_id).await.unwrap_or(0);
    let env = SseEnvelope {
        event_type: "sub_agent_cancelled".into(),
        run_id: run_id.to_string(),
        seq,
        payload: json!({
            "task_key": task_key,
            "reason": reason,
        }),
    };
    streams.publish(session_id, env).await;
}

fn extract_event_text(event: &Event) -> Option<String> {
    event.llm_response.content.as_ref().and_then(|c| {
        if c.parts.iter().any(|p| matches!(p, Part::FunctionCall { .. })) {
            return None;
        }
        let text = c
            .parts
            .iter()
            .filter_map(|p| match p {
                Part::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("");
        if text.is_empty() {
            None
        } else {
            Some(text)
        }
    })
}

fn extract_tool_event(event: &Event) -> Option<Value> {
    let content = event.llm_response.content.as_ref()?;
    for part in &content.parts {
        if let Part::FunctionCall { name, args, id, .. } = part {
            return Some(json!({
                "name": name,
                "args": args,
                "call_id": id,
            }));
        }
    }
    None
}

/// 主 Agent instruction 中关于 spawn 的段落。
pub fn spawn_sub_agent_instruction_block() -> &'static str {
    "\n\
     For multi-step work after update_plan / upsert_todo:\n\
     - Delegate independent implementation steps via spawn_sub_agent(task_key, instruction).\n\
     - One task_key per todo; ensure the todo exists before spawning.\n\
     - After spawn returns, update plan/todos from the summary.\n\
     - Do not spawn the same task_key in parallel.\n\
     - Do not perform long implementation yourself when spawn_sub_agent is appropriate.\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_task_key_normalizes() {
        assert_eq!(sanitize_task_key("step-1"), "step-1");
        assert_eq!(sanitize_task_key("foo/bar"), "foo-bar");
        assert_eq!(sanitize_task_key("---"), "task");
    }

    #[test]
    fn tools_profile_parse() {
        assert_eq!(ToolsProfile::parse(Some("readonly")), ToolsProfile::Readonly);
        assert_eq!(ToolsProfile::parse(Some("full")), ToolsProfile::Full);
        assert_eq!(ToolsProfile::parse(None), ToolsProfile::Coding);
    }
}
