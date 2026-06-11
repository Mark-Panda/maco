use std::path::PathBuf;
use std::sync::Arc;

use adk_core::identity::{SessionId, UserId};
use adk_core::{Content, Event, Part};
use adk_rust::prelude::*;
use futures::StreamExt;
use maco_core::{
    ensure_session_workspace, resolve_project_root, MacoError, MacoResult, ResumeContext,
    SseEnvelope, RUN_STATUS_AWAITING_USER, APP_NAME, USER_ID,
};
use maco_db::{
    CallbackLogRepo, ElicitationRepo, ModelRecord, ReactRepo, SessionMetaRepo, ToolPolicyRecord,
    UsageRepo,
};
use adk_agent::guardrails::{GuardrailSet, PiiRedactor};
use maco_governance::{pii_guardrail_enabled, pricing_from_model, redact_sse_payload, redact_text};
use maco_react::ReactTools;
use maco_storage::{AdkStorage, ArtifactStore};
use maco_telemetry::MacoCallbackLogger;
use tokio::sync::{mpsc, Mutex, RwLock};

use crate::artifact_capture::{
    after_tool_with_artifacts, snapshot_scratch_files, ArtifactCaptureState,
};
use crate::callbacks::{
    after_agent, after_model, before_agent, before_model, before_tool_with_hitl,
};
use crate::elicitation::{
    respond_to_elicitation, ElicitationBroker, ElicitationRunContext, MacoElicitationHandler,
};
use crate::hitl::{build_resume_content, HitlGate};
use crate::mcp_pool::McpPool;
use crate::model_factory::build_llm;
use crate::orchestrator::RunOrchestrator;
use crate::run_stream::RunStreamRegistry;
use crate::shell::MacoBashTool;
use crate::skills::SkillLoader;
use crate::usage::UsageContext;

/// Agent 编排入口：绑定存储、Run 状态机、回调日志与工具策略，驱动一次完整对话 Run。
pub struct MacoHarness {
    storage: Arc<AdkStorage>,
    orchestrator: RunOrchestrator,
    react: ReactRepo,
    callback_logs: CallbackLogRepo,
    usage: UsageRepo,
    elicitation: ElicitationRepo,
    elicitation_broker: ElicitationBroker,
    tool_policies: Arc<RwLock<Vec<ToolPolicyRecord>>>,
    mcp_pool: Arc<McpPool>,
    run_streams: RunStreamRegistry,
    tmp_dir: PathBuf,
    meta: SessionMetaRepo,
    artifacts: Arc<ArtifactStore>,
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
        mcp_pool: Arc<McpPool>,
        run_streams: RunStreamRegistry,
        tmp_dir: PathBuf,
        meta: SessionMetaRepo,
        artifacts: Arc<ArtifactStore>,
    ) -> Self {
        Self {
            storage,
            orchestrator,
            react,
            callback_logs,
            usage,
            elicitation,
            elicitation_broker: ElicitationBroker::new(),
            tool_policies: Arc::new(RwLock::new(tool_policies)),
            mcp_pool,
            run_streams,
            tmp_dir,
            meta,
            artifacts,
        }
    }

    async fn session_project_root(&self, session_id: &str) -> MacoResult<Option<PathBuf>> {
        let meta = self.meta.get(session_id).await?;
        resolve_project_root(meta.as_ref().and_then(|m| m.project_root.as_deref()))
    }

    /// 热更新 HITL 工具策略（影响后续新 Run）。
    pub async fn set_tool_policies(&self, policies: Vec<ToolPolicyRecord>) {
        *self.tool_policies.write().await = policies;
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
        let Some(run_id) = self.run_streams.interrupt(session_id).await else {
            return Ok(None);
        };
        self.orchestrator.cancel_run(&run_id).await?;
        Ok(Some(run_id))
    }

    pub fn orchestrator(&self) -> &RunOrchestrator {
        &self.orchestrator
    }

    pub fn storage(&self) -> &AdkStorage {
        &self.storage
    }

    fn build_instruction(
        &self,
        scratch_dir: &std::path::Path,
        project_root: Option<&std::path::Path>,
    ) -> String {
        let scratch = scratch_dir.display();
        let mut instruction = format!(
            "You are maco, a helpful personal assistant. \
             Use update_plan to maintain a markdown task plan and upsert_todo for actionable items.\n\n\
             Scratch directory (temp downloads, intermediates, throwaway scripts only): `{scratch}`\n\
             - Put temporary/scratch artifacts here — not user project source files.\n",
        );
        if let Some(root) = project_root {
            instruction.push_str(&format!(
                "\nProject root (bound for this session): `{path}`\n\
                 - When editing the user's codebase, use paths under this directory.\n\
                 - bash starts in this directory; relative paths resolve here.\n\
                 - Add this path to MCP filesystem allowed roots if file tools cannot access it.\n",
                path = root.display(),
            ));
        } else {
            instruction.push_str(
                "\nNo project root is bound — ask for the project path or use absolute paths the user provides.\n",
            );
        }
        let skills = SkillLoader::scan(None);
        if !skills.is_empty() {
            instruction.push_str("\n\nAvailable skills:\n");
            for skill in skills.iter().take(8) {
                instruction.push_str(&format!("- {}: {}\n", skill.name, skill.description));
            }
        }
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
    ) -> MacoResult<(String, mpsc::Receiver<SseEnvelope>)> {
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
        let content = build_resume_content(
            &pending.name,
            &pending.call_id,
            approved,
            note,
        );
        let new_run = self
            .orchestrator
            .start_resumed_run(session_id, parent_run_id)
            .await?;
        self.run_with_content(session_id, content, model, Some(new_run.id))
            .await
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
        let project_root = self.session_project_root(session_id).await?;
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

        let llm = build_llm(model)?;
        let react_tools = ReactTools::new(self.react.clone());
        let logger = MacoCallbackLogger::new(
            self.callback_logs.clone(),
            session_id.to_string(),
            run_id.clone(),
        );
        let streams = self.run_streams.clone();
        let hitl = Arc::new(HitlGate {
            run_id: run_id.clone(),
            session_id: session_id.to_string(),
            orchestrator: self.orchestrator.clone(),
            policies: self.tool_policies.read().await.clone(),
            sse_tx: Some(tx.clone()),
            stream: Some(streams.clone()),
        });
        let usage_ctx = Arc::new(UsageContext {
            repo: self.usage.clone(),
            session_id: session_id.to_string(),
            run_id: run_id.clone(),
            model_id: model.id.clone(),
            model_name: model.name.clone(),
            pricing: pricing_from_model(model),
        });
        let artifact_capture = Arc::new(ArtifactCaptureState {
            session_id: session_id.to_string(),
            run_id: run_id.clone(),
            artifacts: Arc::clone(&self.artifacts),
            scratch_dir: scratch_dir.clone(),
            project_root: project_root.clone(),
            scratch_known: Arc::new(Mutex::new(snapshot_scratch_files(&scratch_dir))),
            sse_tx: tx.clone(),
            streams: streams.clone(),
            orchestrator: self.orchestrator.clone(),
        });

        let mut builder = LlmAgentBuilder::new("maco")
            .description("maco personal agent")
            .instruction(self.build_instruction(&scratch_dir, project_root.as_deref()))
            .model(llm)
            .input_guardrails(agent_guardrails())
            .output_guardrails(agent_guardrails())
            .before_callback(before_agent(Arc::clone(&logger)))
            .after_callback(after_agent(Arc::clone(&logger)))
            .before_model_callback(before_model(Arc::clone(&logger)))
            .after_model_callback(after_model(Arc::clone(&logger), Some(usage_ctx)))
            .before_tool_callback(before_tool_with_hitl(Arc::clone(&logger), hitl))
            .after_tool_callback(after_tool_with_artifacts(
                Arc::clone(&logger),
                artifact_capture,
            ));

        for tool in react_tools.as_tool_arcs() {
            builder = builder.tool(tool);
        }
        builder = builder.tool(Arc::new(MacoBashTool::new(
            scratch_dir.clone(),
            project_root.clone(),
        )));
        for ts in self.mcp_pool.toolsets().await {
            builder = builder.toolset(ts);
        }

        let agent = builder
            .build()
            .map_err(|e| MacoError::Adk(e.to_string()))?;

        let runner = Arc::new(
            Runner::builder()
                .app_name(APP_NAME)
                .agent(Arc::new(agent))
                .session_service(self.storage.session_service())
                .memory_service(self.storage.memory_service())
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
        tokio::spawn(async move {
            let run_id = run_id_for_task;
            let mut ok = true;
            while let Some(item) = stream.next().await {
                match item {
                    Ok(event) => {
                        if let Ok(seq) = orchestrator.next_seq(&run_id).await {
                            let text = redact_text(&extract_event_text(&event));
                            if !text.is_empty() {
                                publish_sse(
                                    &streams_task,
                                    &session_id_task,
                                    &tx,
                                    SseEnvelope {
                                        event_type: "text".into(),
                                        run_id: run_id.clone(),
                                        seq,
                                        payload: serde_json::json!({ "content": text }),
                                    },
                                )
                                .await;
                            }
                            if let Some(tool_ev) = extract_tool_event(&event) {
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
                        if r.status != RUN_STATUS_AWAITING_USER {
                            let _ = orchestrator.complete_run(&run_id).await;
                        }
                    }
                }
                if let Ok(seq) = orchestrator.next_seq(&run_id).await {
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

fn agent_guardrails() -> GuardrailSet {
    let mut set = GuardrailSet::new();
    if pii_guardrail_enabled() {
        set = set.with(PiiRedactor::new());
    }
    set
}

fn extract_event_text(event: &Event) -> String {
    event
        .llm_response
        .content
        .as_ref()
        .map(|c| {
            c.parts
                .iter()
                .filter_map(|p| match p {
                    Part::Text { text } => Some(text.as_str()),
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
