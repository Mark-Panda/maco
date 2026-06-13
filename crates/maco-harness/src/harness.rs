use std::path::PathBuf;
use std::sync::Arc;

use adk_core::identity::{SessionId, UserId};
use adk_core::{Content, Event, Part, Tool, ToolRegistry};
use adk_rust::prelude::*;
use adk_tool::{LoadArtifactsTool, SimpleToolContext};
use futures::StreamExt;
use maco_core::{
    branch_name, ensure_session_workspace, resolve_session_workspace, workspace_from_cached,
    AgentPermissionMode, MacoError, MacoResult, PendingToolCall, ResumeContext, SessionWorkspace,
    SseEnvelope, RUN_STATUS_AWAITING_USER, APP_NAME, USER_ID,
};
use maco_db::{
    CallbackLogRepo, ElicitationRepo, ModelRecord, ReactRepo, SessionMetaRepo,
    SubAgentRunRepo, ToolPolicyRecord, UsageRepo,
};
use maco_governance::{pricing_from_model, redact_sse_payload, redact_text};
use maco_react::ReactTools;
use maco_storage::{adk_artifacts_enabled, AdkStorage, ArtifactStore};
use maco_telemetry::MacoCallbackLogger;
use tokio::sync::{mpsc, Mutex, RwLock};

use crate::artifact_capture::{
    after_tool_with_artifacts, snapshot_scratch_files, ArtifactCaptureState,
};
use crate::callbacks::{
    after_agent, agent_guardrails, before_agent, before_tool_with_hitl,
};
use crate::model_activity::{
    after_model_with_activity, before_model_with_activity, ModelActivityState,
};
use crate::sub_agent::{
    emit_sub_agent_cancelled, sanitize_task_key, spawn_sub_agent_instruction_block,
    SubAgentCoordinator, SubAgentParentBridge, SubAgentRunContext,
};
use crate::filesystem_mcp::FilesystemMcpCoordinator;
use crate::elicitation::{
    respond_to_elicitation, ElicitationBroker, ElicitationRunContext, MacoElicitationHandler,
};
use crate::hitl::{build_resume_content, build_tool_result_content, HitlBroker, HitlGate};
use crate::mcp_pool::McpPool;
use crate::model_factory::{build_llm_for_run, max_tokens_for_model};
use crate::orchestrator::RunOrchestrator;
use crate::run_stream::RunStreamRegistry;
use crate::shell::MacoBashTool;
use crate::adk_skills::AdkSkillManager;
use crate::compaction::{compaction_enabled, runner_compaction_options};
use crate::tool_concurrency::{runner_run_config, tool_concurrency_enabled};
use crate::skill_coordinator::{
    default_coordinator_config, extract_user_query, resolve_skill_context,
    skill_restricts_tools, MacoToolRegistry,
};
use crate::usage::UsageContext;

/// HITL 恢复结果：同 Run 内唤醒，或断线后新建 Run 并返回 SSE。
pub enum ResumeHitlOutcome {
    /// 已在原 Run 内继续，无需新 SSE 流。
    InPlace,
    /// 断线 fallback：新 Run 的事件流。
    Stream {
        run_id: String,
        rx: mpsc::Receiver<SseEnvelope>,
    },
}

/// Agent 编排入口：绑定存储、Run 状态机、回调日志与工具策略，驱动一次完整对话 Run。
pub struct MacoHarness {
    storage: Arc<AdkStorage>,
    orchestrator: RunOrchestrator,
    react: ReactRepo,
    callback_logs: CallbackLogRepo,
    usage: UsageRepo,
    elicitation: ElicitationRepo,
    elicitation_broker: ElicitationBroker,
    hitl_broker: HitlBroker,
    tool_policies: Arc<RwLock<Vec<ToolPolicyRecord>>>,
    worktree_path_guard: Arc<RwLock<bool>>,
    mcp_pool: Arc<McpPool>,
    filesystem_mcp: Arc<FilesystemMcpCoordinator>,
    run_streams: RunStreamRegistry,
    tmp_dir: PathBuf,
    meta: SessionMetaRepo,
    artifacts: Arc<ArtifactStore>,
    adk_skills: Arc<AdkSkillManager>,
    sub_agent_runs: SubAgentRunRepo,
    sub_agent_coordinator: SubAgentCoordinator,
}

impl MacoHarness {
    pub fn new(
        storage: Arc<AdkStorage>,
        orchestrator: RunOrchestrator,
        react: ReactRepo,
        callback_logs: CallbackLogRepo,
        usage: UsageRepo,
        elicitation: ElicitationRepo,
        tool_policies: Vec<ToolPolicyRecord>,
        worktree_path_guard: bool,
        mcp_pool: Arc<McpPool>,
        filesystem_mcp: Arc<FilesystemMcpCoordinator>,
        elicitation_broker: ElicitationBroker,
        run_streams: RunStreamRegistry,
        tmp_dir: PathBuf,
        meta: SessionMetaRepo,
        artifacts: Arc<ArtifactStore>,
        adk_skills: Arc<AdkSkillManager>,
        sub_agent_runs: SubAgentRunRepo,
    ) -> Self {
        Self {
            storage,
            orchestrator,
            react,
            callback_logs,
            usage,
            elicitation,
            elicitation_broker,
            hitl_broker: HitlBroker::new(),
            tool_policies: Arc::new(RwLock::new(tool_policies)),
            worktree_path_guard: Arc::new(RwLock::new(worktree_path_guard)),
            mcp_pool,
            filesystem_mcp,
            run_streams,
            tmp_dir,
            meta,
            artifacts,
            adk_skills,
            sub_agent_runs,
            sub_agent_coordinator: SubAgentCoordinator::new(),
        }
    }

    pub fn adk_skills(&self) -> &Arc<AdkSkillManager> {
        &self.adk_skills
    }

    async fn session_workspace(&self, session_id: &str) -> MacoResult<Option<SessionWorkspace>> {
        let meta = self.meta.get(session_id).await?;
        let Some(rec) = meta else {
            return Ok(None);
        };
        let worktree_enabled = rec.git_worktree_enabled != 0;
        let cached_ok = if worktree_enabled && rec.project_root.is_some() {
            let path_valid = rec
                .git_worktree_path
                .as_deref()
                .filter(|p| !p.trim().is_empty())
                .map(std::path::Path::new)
                .is_some_and(|p| p.exists());
            let expected_branch = branch_name(&rec.git_branch_prefix, session_id);
            let branch_ok = rec.git_worktree_branch.as_deref() == Some(expected_branch.as_str());
            path_valid && branch_ok
        } else {
            !worktree_enabled
        };
        if cached_ok {
            if let Some(ws) = workspace_from_cached(
                rec.project_root.as_deref(),
                worktree_enabled,
                rec.git_worktree_path.as_deref(),
                rec.git_worktree_branch.as_deref(),
            )? {
                if !worktree_enabled || ws.uses_worktree {
                    return Ok(Some(ws));
                }
            }
        }
        let ws = resolve_session_workspace(
            rec.project_root.as_deref(),
            session_id,
            rec.git_worktree_enabled != 0,
            &rec.git_branch_prefix,
        )?;
        if let Some(ref workspace) = ws {
            if workspace.uses_worktree {
                let branch = workspace.worktree_branch.as_deref().unwrap_or("");
                self.meta
                    .update_worktree_state(
                        session_id,
                        &workspace.workspace_root.to_string_lossy(),
                        branch,
                    )
                    .await?;
            } else {
                self.meta.clear_worktree_state(session_id).await?;
            }
        }
        Ok(ws)
    }

    async fn session_permission_mode(&self, session_id: &str) -> AgentPermissionMode {
        self.meta
            .get(session_id)
            .await
            .ok()
            .flatten()
            .map(|m| AgentPermissionMode::parse(&m.permission_mode))
            .unwrap_or_default()
    }

    /// 热更新 HITL 工具策略（影响后续新 Run）。
    pub async fn set_tool_policies(&self, policies: Vec<ToolPolicyRecord>) {
        *self.tool_policies.write().await = policies;
    }

    pub async fn worktree_path_guard_enabled(&self) -> bool {
        *self.worktree_path_guard.read().await
    }

    pub async fn set_worktree_path_guard(&self, enabled: bool) {
        *self.worktree_path_guard.write().await = enabled;
    }

    pub fn elicitation_broker(&self) -> &ElicitationBroker {
        &self.elicitation_broker
    }

    pub fn run_streams(&self) -> &RunStreamRegistry {
        &self.run_streams
    }

    pub fn mcp_pool(&self) -> &Arc<McpPool> {
        &self.mcp_pool
    }

    pub fn create_elicitation_handler(
        &self,
        session_id: &str,
        run_id: &str,
        mcp_server: &str,
        sse_tx: Option<mpsc::Sender<SseEnvelope>>,
    ) -> Arc<MacoElicitationHandler> {
        Arc::new(MacoElicitationHandler {
            session_id: session_id.to_string(),
            run_id: run_id.to_string(),
            mcp_server: mcp_server.to_string(),
            orchestrator: self.orchestrator.clone(),
            repo: self.elicitation.clone(),
            broker: self.elicitation_broker.clone(),
            sse_tx,
            stream: Some(self.run_streams.clone()),
        })
    }

    pub async fn respond_elicitation(
        &self,
        elicitation_id: &str,
        action: rmcp::model::ElicitationAction,
        content: Option<serde_json::Value>,
    ) -> MacoResult<bool> {
        respond_to_elicitation(
            &self.elicitation,
            &self.elicitation_broker,
            elicitation_id,
            action,
            content,
        )
        .await
    }

    /// 中断会话上活跃的 Runner，并将 Run 标为 cancelled。
    pub async fn interrupt_session(&self, session_id: &str) -> MacoResult<Option<String>> {
        if let Some(run_id) = self.run_streams.active_run_id(session_id).await {
            self.cascade_cancel_sub_agents(session_id, &run_id).await?;
        }
        let Some(run_id) = self.run_streams.interrupt(session_id).await else {
            return Ok(None);
        };
        self.hitl_broker.cancel(&run_id).await;
        if let Ok(pending) = self.elicitation.list_pending_for_run(&run_id).await {
            for rec in pending {
                self.elicitation_broker.cancel(&rec.id).await;
                let _ = self.elicitation.mark_expired(&rec.id).await;
            }
        }
        let _ = self.filesystem_mcp.force_end_session_scope(session_id).await;
        self.orchestrator.cancel_run(&run_id).await?;
        Ok(Some(run_id))
    }

    /// 取消指定 parent run 下正在执行的子 Agent。
    pub async fn cancel_sub_agent(
        &self,
        session_id: &str,
        parent_run_id: &str,
        task_key: &str,
    ) -> MacoResult<bool> {
        let task_key = sanitize_task_key(task_key);
        if let Some((tk, _)) = self
            .sub_agent_coordinator
            .cancel(parent_run_id, &task_key)
            .await
        {
            emit_sub_agent_cancelled(
                &self.run_streams,
                &self.orchestrator,
                session_id,
                parent_run_id,
                &tk,
                "user_cancel",
            )
            .await;
            return Ok(true);
        }
        Ok(false)
    }

    async fn cascade_cancel_sub_agents(
        &self,
        session_id: &str,
        parent_run_id: &str,
    ) -> MacoResult<()> {
        if let Some((task_key, _)) = self
            .sub_agent_coordinator
            .cancel_all_for_run(parent_run_id)
            .await
        {
            emit_sub_agent_cancelled(
                &self.run_streams,
                &self.orchestrator,
                session_id,
                parent_run_id,
                &task_key,
                "parent_run_interrupted",
            )
            .await;
        }
        Ok(())
    }

    pub fn sub_agent_runs(&self) -> &SubAgentRunRepo {
        &self.sub_agent_runs
    }

    pub fn orchestrator(&self) -> &RunOrchestrator {
        &self.orchestrator
    }

    pub fn storage(&self) -> &AdkStorage {
        &self.storage
    }

    fn filesystem_allowed_roots(
        scratch_dir: &std::path::Path,
        workspace: Option<&SessionWorkspace>,
    ) -> Vec<String> {
        let mut roots = vec![scratch_dir.to_string_lossy().into_owned()];
        if let Some(ws) = workspace {
            roots.push(ws.workspace_root.to_string_lossy().into_owned());
        }
        roots
    }

    fn build_instruction(
        &self,
        scratch_dir: &std::path::Path,
        workspace: Option<&SessionWorkspace>,
    ) -> String {
        let scratch = scratch_dir.display();
        let mut instruction = format!(
            "You are maco, a helpful personal assistant. \
             For multi-step tasks you MUST manage plan/todos incrementally — never defer to the end:\n\
             1) At the START, call update_plan with an unchecked markdown checklist (`- [ ] step`).\n\
             2) Immediately call upsert_todo for each step (status `pending`).\n\
             3) Before starting a step, mark it `in_progress` via upsert_todo and update_plan (`- [~]`).\n\
             4) After finishing a step, mark it `completed` via upsert_todo and update_plan (`- [x]`).\n\
             Keep plan checkboxes and todo status in sync throughout the run.\n\
             Reply in the same language as the user (if the user writes in Chinese, respond in Chinese). \
             Do not mix languages in one reply except for code, paths, or technical terms.\n\n\
             Scratch directory (temp downloads, intermediates, throwaway scripts only): `{scratch}`\n\
             - Put temporary/scratch artifacts here — not user project source files.\n",
        );
        if let Some(ws) = workspace {
            if ws.uses_worktree {
                let branch = ws
                    .worktree_branch
                    .as_deref()
                    .unwrap_or("(unknown)");
                instruction.push_str(&format!(
                    "\nGit worktree workspace (MUST edit here): `{wt}`\n\
                     Worktree branch: `{branch}`\n\
                     Repository root (do NOT edit directly): `{repo}`\n\
                     - All code changes MUST stay inside the worktree workspace directory.\n\
                     - bash starts in the worktree; relative paths resolve there.\n\
                     - Never modify files in the main repository checkout.\n\
                     - Custom MCP tools with file path arguments are also blocked from targeting the main repository.\n",
                    wt = ws.workspace_root.display(),
                    repo = ws.repo_root.display(),
                ));
            } else {
                instruction.push_str(&format!(
                    "\nProject root (bound for this session): `{path}`\n\
                     - When editing the user's codebase, use paths under this directory.\n\
                     - bash starts in this directory; relative paths resolve here.\n",
                    path = ws.workspace_root.display(),
                ));
            }
        } else {
            instruction.push_str(
                "\nNo project root is bound — ask for the project path or use absolute paths the user provides.\n",
            );
        }
        instruction.push_str(
            "\nRelevant skills from `.skills/`, `.claude/skills/`, or `~/.maco/skills/` \
             are resolved via ADK ContextCoordinator; `allowed-tools` in frontmatter bind only those tools.\n",
        );
        instruction.push_str(spawn_sub_agent_instruction_block());
        instruction
    }

    pub async fn run_chat(
        &self,
        session_id: &str,
        user_text: &str,
        model: &ModelRecord,
    ) -> MacoResult<(String, mpsc::Receiver<SseEnvelope>)> {
        self.run_with_content(session_id, user_text_content(user_text), model, None)
            .await
    }

    pub async fn resume_run(
        &self,
        session_id: &str,
        parent_run_id: &str,
        approved: bool,
        note: Option<&str>,
        model: &ModelRecord,
    ) -> MacoResult<ResumeHitlOutcome> {
        if self.hitl_broker.fulfill(parent_run_id, approved).await {
            return Ok(ResumeHitlOutcome::InPlace);
        }
        self.resume_run_fallback(session_id, parent_run_id, approved, note, model)
            .await
    }

    async fn resume_run_fallback(
        &self,
        session_id: &str,
        parent_run_id: &str,
        approved: bool,
        note: Option<&str>,
        model: &ModelRecord,
    ) -> MacoResult<ResumeHitlOutcome> {
        let parent = self
            .orchestrator
            .get_run(parent_run_id)
            .await?
            .ok_or_else(|| MacoError::not_found("run"))?;
        let resume_raw = parent
            .resume_context
            .as_deref()
            .ok_or_else(|| MacoError::conflict("missing resume_context"))?;
        let resume: ResumeContext = serde_json::from_str(resume_raw)
            .map_err(|e| MacoError::config(format!("invalid resume_context: {e}")))?;
        let pending = resume
            .pending_tool_call
            .as_ref()
            .ok_or_else(|| MacoError::config("resume_context missing pending_tool_call"))?;
        let content = if approved {
            let result = self
                .execute_pending_tool(session_id, pending)
                .await?;
            build_tool_result_content(&pending.name, &pending.call_id, result)
        } else {
            build_resume_content(&pending.name, &pending.call_id, false, note)
        };
        let new_run = self
            .orchestrator
            .start_resumed_run(session_id, parent_run_id)
            .await?;
        let (run_id, rx) = self
            .run_with_content(session_id, content, model, Some(new_run.id))
            .await?;
        Ok(ResumeHitlOutcome::Stream { run_id, rx })
    }

    async fn execute_pending_tool(
        &self,
        session_id: &str,
        pending: &PendingToolCall,
    ) -> MacoResult<serde_json::Value> {
        let scratch_dir = ensure_session_workspace(&self.tmp_dir, session_id)?;
        let workspace = self.session_workspace(session_id).await?;
        let roots = Self::filesystem_allowed_roots(&scratch_dir, workspace.as_ref());
        let session_fs = self
            .filesystem_mcp
            .acquire_for_session(session_id, &roots)
            .await?;
        let mut mcp_toolsets = vec![session_fs.toolset()];
        mcp_toolsets.extend(self.mcp_pool.toolsets().await);
        let worktree_path_guard = self.worktree_path_guard_enabled().await;
        let react_tools = ReactTools::new(self.react.clone());
        let bash_tool: Arc<dyn Tool> = Arc::new(MacoBashTool::new(
            scratch_dir,
            workspace,
            worktree_path_guard,
        ));
        let registry = MacoToolRegistry::build(&react_tools, bash_tool, &mcp_toolsets).await?;
        let tool = registry
            .resolve(&pending.name)
            .ok_or_else(|| MacoError::conflict(format!(
                "tool `{}` is not available after reconnect",
                pending.name
            )))?;
        tool.execute(
            Arc::new(SimpleToolContext::new(&pending.call_id)),
            pending.args.clone(),
        )
        .await
        .map_err(|e| MacoError::Adk(e.to_string()))
    }

    async fn run_with_content(
        &self,
        session_id: &str,
        content: Content,
        model: &ModelRecord,
        existing_run_id: Option<String>,
    ) -> MacoResult<(String, mpsc::Receiver<SseEnvelope>)> {
        let run = if let Some(id) = existing_run_id {
            self.orchestrator
                .get_run(&id)
                .await?
                .ok_or_else(|| MacoError::run("run missing"))?
        } else {
            self.orchestrator.start_run(session_id).await?
        };
        let scratch_dir = ensure_session_workspace(&self.tmp_dir, session_id)?;
        let workspace = self.session_workspace(session_id).await?;
        let roots = Self::filesystem_allowed_roots(&scratch_dir, workspace.as_ref());
        let session_fs = self
            .filesystem_mcp
            .acquire_for_session(session_id, &roots)
            .await?;
        let mut mcp_toolsets = vec![session_fs.toolset()];
        mcp_toolsets.extend(self.mcp_pool.toolsets().await);
        let skill_root = workspace.as_ref().map(|ws| ws.workspace_root.as_path());
        if let Err(e) = self.adk_skills.reload_from_disk(skill_root) {
            tracing::warn!("reload adk skills: {e}");
        }
        let skill_index = self.adk_skills.agent_index();
        let worktree_path_guard = self.worktree_path_guard_enabled().await;
        let react_tools = ReactTools::new(self.react.clone());
        let bash_tool: Arc<dyn Tool> = Arc::new(MacoBashTool::new(
            scratch_dir.clone(),
            workspace.clone(),
            worktree_path_guard,
        ));
        let tool_registry = Arc::new(
            MacoToolRegistry::build(&react_tools, bash_tool.clone(), &mcp_toolsets).await?,
        );
        let user_query = extract_user_query(&content);
        let skill_context = resolve_skill_context(
            &skill_index,
            tool_registry.clone(),
            &user_query,
            &default_coordinator_config(),
        );
        let run_id = run.id.clone();
        let run_id_for_task = run_id.clone();
        let (tx, rx) = mpsc::channel(256);

        self.mcp_pool
            .elicitation()
            .set_run_context(ElicitationRunContext {
                session_id: session_id.to_string(),
                run_id: run_id.clone(),
                mcp_server: "mcp".into(),
                sse_tx: Some(tx.clone()),
                session_broadcast: Some(session_id.to_string()),
            })
            .await;

        let llm = build_llm_for_run(model, Some(session_id), Some(&run_id))?;
        let max_output_tokens = max_tokens_for_model(model) as i32;
        let compaction_opts = compaction_enabled().then(|| runner_compaction_options(llm.clone()));
        let logger = MacoCallbackLogger::new(
            self.callback_logs.clone(),
            session_id.to_string(),
            run_id.clone(),
        );
        let streams = self.run_streams.clone();
        let permission_mode = self.session_permission_mode(session_id).await;
        let hitl = Arc::new(HitlGate {
            run_id: run_id.clone(),
            session_id: session_id.to_string(),
            orchestrator: self.orchestrator.clone(),
            policies: self.tool_policies.read().await.clone(),
            permission_mode,
            sse_tx: Some(tx.clone()),
            stream: Some(streams.clone()),
            broker: self.hitl_broker.clone(),
        });
        let usage_ctx = Arc::new(UsageContext {
            repo: self.usage.clone(),
            session_id: session_id.to_string(),
            run_id: run_id.clone(),
            model_id: model.id.clone(),
            model_name: model.name.clone(),
            pricing: pricing_from_model(model),
        });
        let workspace_root = workspace
            .as_ref()
            .map(|ws| ws.workspace_root.clone());
        let artifact_capture = Arc::new(ArtifactCaptureState {
            session_id: session_id.to_string(),
            run_id: run_id.clone(),
            artifacts: Arc::clone(&self.artifacts),
            scratch_dir: scratch_dir.clone(),
            project_root: workspace_root,
            scratch_known: Arc::new(Mutex::new(snapshot_scratch_files(&scratch_dir))),
            sse_tx: tx.clone(),
            streams: streams.clone(),
            orchestrator: self.orchestrator.clone(),
        });
        let model_activity = Arc::new(ModelActivityState::new(
            session_id.to_string(),
            run_id.clone(),
            tx.clone(),
            streams.clone(),
            self.orchestrator.clone(),
        ));

        let mut instruction = self.build_instruction(&scratch_dir, workspace.as_ref());
        if let Some(ref skill_ctx) = skill_context {
            instruction.push_str("\n\n## Active Skill\n\n");
            instruction.push_str(&skill_ctx.system_instruction);
        }

        let sub_agent_parent_bridge = SubAgentParentBridge::new();

        let sub_agent_ctx = SubAgentRunContext::new(
            session_id.to_string(),
            run_id.clone(),
            Arc::clone(&llm),
            max_output_tokens,
            scratch_dir.clone(),
            workspace.clone(),
            worktree_path_guard,
            bash_tool.clone(),
            mcp_toolsets.clone(),
            self.react.clone(),
            self.storage.session_service(),
            self.storage.memory_service(),
            Arc::clone(&logger),
            Arc::clone(&hitl),
            Some(Arc::clone(&usage_ctx)),
            streams.clone(),
            tx.clone(),
            self.orchestrator.clone(),
            self.sub_agent_runs.clone(),
            model.id.clone(),
            self.sub_agent_coordinator.clone(),
            sub_agent_parent_bridge.clone(),
        );

        let mut builder = LlmAgentBuilder::new("maco")
            .description("maco personal agent")
            .instruction(instruction)
            .model(llm)
            .max_output_tokens(max_output_tokens)
            .input_guardrails(agent_guardrails())
            .output_guardrails(agent_guardrails())
            .before_callback(before_agent(Arc::clone(&logger)))
            .after_callback(after_agent(Arc::clone(&logger)))
            .before_model_callback(before_model_with_activity(
                Arc::clone(&logger),
                Arc::clone(&model_activity),
            ))
            .after_model_callback(after_model_with_activity(
                Arc::clone(&logger),
                Some(usage_ctx.clone()),
                Arc::clone(&model_activity),
            ))
            .before_tool_callback(before_tool_with_hitl(
                Arc::clone(&logger),
                hitl,
                workspace.clone(),
                worktree_path_guard,
            ))
            .after_tool_callback(after_tool_with_artifacts(
                Arc::clone(&logger),
                artifact_capture,
            ));

        if skill_context
            .as_ref()
            .is_some_and(skill_restricts_tools)
        {
            if let Some(ref skill_ctx) = skill_context {
                for tool in &skill_ctx.active_tools {
                    builder = builder.tool(tool.clone());
                }
            }
        } else {
            for tool in react_tools.as_tool_arcs() {
                builder = builder.tool(tool);
            }
            builder = builder.tool(sub_agent_ctx.spawn_tool());
            builder = builder.tool(bash_tool);
            for ts in mcp_toolsets {
                builder = builder.toolset(ts);
            }
        }

        if adk_artifacts_enabled() {
            builder = builder.tool(Arc::new(LoadArtifactsTool::new()));
        }

        let agent = builder
            .build()
            .map_err(|e| MacoError::Adk(e.to_string()))?;

        let mut runner_builder = Runner::builder()
            .app_name(APP_NAME)
            .agent(Arc::new(agent))
            .session_service(self.storage.session_service())
            .memory_service(self.storage.memory_service());

        if let Some(compaction) = compaction_opts {
            runner_builder = runner_builder
                .compaction_config(compaction.events)
                .intra_compaction_config(compaction.intra)
                .intra_compaction_summarizer(compaction.intra_summarizer)
                .context_compaction(compaction.overflow);
            tracing::debug!("runner context compaction enabled");
        }

        if tool_concurrency_enabled() {
            runner_builder = runner_builder.run_config(runner_run_config());
            tracing::debug!("runner tool concurrency enabled");
        }

        if adk_artifacts_enabled() {
            runner_builder =
                runner_builder.artifact_service(self.artifacts.adk_service());
            tracing::debug!("runner ADK artifact service enabled");
        }

        let runner = Arc::new(
            runner_builder
                .build()
                .map_err(|e| MacoError::Adk(e.to_string()))?,
        );

        let _btx = self
            .run_streams
            .register(session_id, run_id.clone(), runner.clone())
            .await;

        let user_id = UserId::try_from(USER_ID).map_err(|e| MacoError::Adk(e.to_string()))?;
        let sid = SessionId::try_from(session_id).map_err(|e| MacoError::Adk(e.to_string()))?;

        let mut stream = runner
            .run(user_id, sid, content)
            .await
            .map_err(|e| MacoError::Adk(e.to_string()))?;

        let orchestrator = self.orchestrator.clone();
        let session_id_task = session_id.to_string();
        let streams_task = self.run_streams.clone();
        let filesystem_mcp_task = self.filesystem_mcp.clone();
        tokio::spawn(async move {
            let _session_fs = session_fs;
            let run_id = run_id_for_task;
            let mut ok = true;
            let mut awaiting_user = false;
            let mut last_emitted_text = String::new();
            while let Some(item) = stream.next().await {
                match item {
                    Ok(event) => {
                        if let Ok(seq) = orchestrator.next_seq(&run_id).await {
                            if let Some(tool_ev) = extract_tool_event(&event) {
                                last_emitted_text.clear();
                                publish_sse(
                                    &streams_task,
                                    &session_id_task,
                                    &tx,
                                    SseEnvelope {
                                        event_type: "tool_call".into(),
                                        run_id: run_id.clone(),
                                        seq,
                                        payload: tool_ev,
                                    },
                                )
                                .await;
                            } else {
                                let text = redact_text(&extract_event_text(&event));
                                if !text.is_empty() {
                                    let delta = compute_text_delta(&mut last_emitted_text, &text);
                                    if !delta.is_empty() {
                                        publish_sse(
                                            &streams_task,
                                            &session_id_task,
                                            &tx,
                                            SseEnvelope {
                                                event_type: "text".into(),
                                                run_id: run_id.clone(),
                                                seq,
                                                payload: serde_json::json!({ "content": delta }),
                                            },
                                        )
                                        .await;
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        ok = false;
                        let _ = orchestrator.fail_run(&run_id, &e.to_string()).await;
                        let mut err_payload =
                            serde_json::json!({ "message": e.to_string() });
                        redact_sse_payload(&mut err_payload);
                        publish_sse(
                            &streams_task,
                            &session_id_task,
                            &tx,
                            SseEnvelope {
                                event_type: "error".into(),
                                run_id: run_id.clone(),
                                seq: 0,
                                payload: err_payload,
                            },
                        )
                        .await;
                        break;
                    }
                }
            }
            if ok {
                if let Ok(current) = orchestrator.get_run(&run_id).await {
                    if let Some(r) = current {
                        awaiting_user = r.status == RUN_STATUS_AWAITING_USER;
                        if !awaiting_user {
                            let _ = orchestrator.complete_run(&run_id).await;
                        }
                    }
                }
                if let Ok(seq) = orchestrator.next_seq(&run_id).await {
                    if awaiting_user {
                        publish_sse(
                            &streams_task,
                            &session_id_task,
                            &tx,
                            SseEnvelope {
                                event_type: "awaiting_user".into(),
                                run_id: run_id.clone(),
                                seq,
                                payload: serde_json::json!({ "status": RUN_STATUS_AWAITING_USER }),
                            },
                        )
                        .await;
                    } else {
                        publish_sse(
                            &streams_task,
                            &session_id_task,
                            &tx,
                            SseEnvelope {
                                event_type: "done".into(),
                                run_id: run_id.clone(),
                                seq,
                                payload: serde_json::json!({}),
                            },
                        )
                        .await;
                    }
                }
            }
            if !awaiting_user {
                filesystem_mcp_task.release_session(&session_id_task).await;
            }
            streams_task.unregister(&session_id_task).await;
        });

        Ok((run_id, rx))
    }
}

async fn publish_sse(
    streams: &RunStreamRegistry,
    session_id: &str,
    mpsc_tx: &mpsc::Sender<SseEnvelope>,
    env: SseEnvelope,
) {
    let _ = mpsc_tx.send(env.clone()).await;
    streams.publish(session_id, env).await;
}

fn user_text_content(user_text: &str) -> Content {
    Content {
        role: "user".into(),
        parts: vec![Part::Text {
            text: user_text.to_string(),
        }],
    }
}

fn compute_text_delta(last_emitted: &mut String, text: &str) -> String {
    if text.starts_with(last_emitted.as_str()) {
        let delta = text[last_emitted.len()..].to_string();
        *last_emitted = text.to_string();
        return delta;
    }
    if last_emitted.starts_with(text) {
        *last_emitted = text.to_string();
        return String::new();
    }
    let delta = text.to_string();
    *last_emitted = text.to_string();
    delta
}

fn extract_event_text(event: &Event) -> String {
    event
        .llm_response
        .content
        .as_ref()
        .map(|c| {
            if c.parts.iter().any(|p| matches!(p, Part::FunctionCall { .. })) {
                return String::new();
            }
            c.parts
                .iter()
                .filter_map(|p| match p {
                    Part::Text { text } => Some(text.as_str()),
                    Part::Thinking { .. } => None,
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default()
}

fn extract_tool_event(event: &Event) -> Option<serde_json::Value> {
    let content = event.llm_response.content.as_ref()?;
    for part in &content.parts {
        if let Part::FunctionCall { name, args, id, .. } = part {
            return Some(serde_json::json!({
                "name": name,
                "args": args,
                "call_id": id,
            }));
        }
    }
    None
}
