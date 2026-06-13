//! 应用全局状态：数据库仓库、Harness、MCP 池与 Session 门面。

use std::path::Path;
use std::sync::Arc;

use maco_db::{
    ApiTokenRepo, ArtifactRepo, CallbackLogRepo, ElicitationRepo, JobRepo, McpServerRepo,
    ModelRepo, ReactRepo, RunEventRepo, RunRepo, SessionMetaRepo, SettingsRepo, SkillRepo,
    SubAgentRunRepo, ToolPolicyRecord, ToolPolicyRepo, UsageRepo, init_pool,
    seed_default_filesystem_mcp, seed_defaults, seed_tool_policies, worktree_path_guard_enabled,
};
use maco_governance::auth_disabled;
use maco_harness::{
    AdkSkillManager, DynamicElicitationHandler, ElicitationBroker, FilesystemMcpCoordinator,
    MacoHarness, McpPool, RunOrchestrator, RunStreamRegistry,
};
use maco_storage::{AdkStorage, ArtifactStore};

use crate::session_facade::SessionFacade;
use crate::skills_sync;

/// Axum `State` 注入的共享上下文（各 HTTP handler 通过 `State<AppState>` 访问）。
#[derive(Clone)]
pub struct AppState {
    pub runtime: RuntimeContext,
    pub repos: RepoContext,
    pub agent: AgentContext,
    pub storage: StorageContext,
}

/// 进程级运行参数。
#[derive(Clone)]
pub struct RuntimeContext {
    /// HTTP 监听地址（健康检查回显用）。
    pub bind_addr: String,
    /// 是否关闭 Bearer 鉴权。
    pub auth_disabled: bool,
    /// Agent 临时目录根路径。
    pub tmp_dir: std::path::PathBuf,
}

/// 业务数据库仓库集合。
#[derive(Clone)]
pub struct RepoContext {
    /// 会话元数据仓库。
    pub meta: SessionMetaRepo,
    /// 模型配置仓库。
    pub models: ModelRepo,
    /// Run 状态仓库。
    pub runs: RunRepo,
    /// Run SSE 事件回放仓库。
    pub run_events: RunEventRepo,
    /// ReAct plan/todo 仓库。
    pub react: ReactRepo,
    /// API Token 仓库。
    pub api_tokens: ApiTokenRepo,
    /// 用量统计仓库。
    pub usage: UsageRepo,
    /// Elicitation 仓库。
    pub elicitation: ElicitationRepo,
    /// 后台任务仓库。
    pub jobs: JobRepo,
    /// MCP 服务配置仓库。
    pub mcp_servers: McpServerRepo,
    /// HITL 工具策略仓库。
    pub tool_policies: ToolPolicyRepo,
    /// 全局应用设置。
    pub settings: SettingsRepo,
    /// Skill 元数据与启用状态。
    pub skills: SkillRepo,
    /// 子 Agent spawn 审计。
    pub sub_agent_runs: SubAgentRunRepo,
}

/// Agent/ADK 运行时服务集合。
#[derive(Clone)]
pub struct AgentContext {
    /// ADK Skill 索引与启用状态。
    pub adk_skills: Arc<AdkSkillManager>,
    /// adk Session/Memory 存储。
    pub adk: Arc<AdkStorage>,
    /// Session + Memory 门面。
    pub facade: Arc<SessionFacade>,
    /// Agent 编排 Harness。
    pub harness: Arc<MacoHarness>,
    /// MCP 连接池。
    pub mcp_pool: Arc<McpPool>,
    /// filesystem MCP 根目录与 Run 生命周期协调。
    pub filesystem_mcp: Arc<FilesystemMcpCoordinator>,
}

/// 文件与附件存储服务集合。
#[derive(Clone)]
pub struct StorageContext {
    /// 附件存储。
    pub artifacts: Arc<ArtifactStore>,
}

impl AppState {
    /// 工作区变更后丢弃会话级 filesystem MCP 缓存，下次 Run 按新根目录启动子进程。
    pub async fn invalidate_session_filesystem_cache(&self, session_id: &str) {
        self.agent.filesystem_mcp.release_session(session_id).await;
    }

    /// Agent Run 期间禁止重载 MCP 连接池。
    pub async fn reload_mcp_pool_guarded(&self) -> maco_core::MacoResult<()> {
        if self.agent.harness.run_streams().has_active().await {
            return Err(maco_core::MacoError::conflict(
                "cannot reload MCP while an agent run is active",
            ));
        }
        self.agent.mcp_pool.reload().await?;
        Ok(())
    }

    /// 初始化连接池、迁移、默认种子、adk 存储、Harness 与启动对账。
    pub async fn new(
        bind_addr: String,
        db_path: &std::path::Path,
        paths: &maco_core::DataPaths,
    ) -> maco_core::MacoResult<Self> {
        let db = init_pool(db_path).await?;
        let settings = SettingsRepo::new(db.pool.clone());
        let tool_policies_repo = ToolPolicyRepo::new(db.pool.clone());
        let mcp_servers = McpServerRepo::new(db.pool.clone());
        let policies =
            seed_application_defaults(&settings, &tool_policies_repo, &mcp_servers, &paths.tmp_dir)
                .await?;

        let callback_logs = CallbackLogRepo::new(db.pool.clone());

        let adk = Arc::new(AdkStorage::open(paths).await?);
        let meta = SessionMetaRepo::new(db.pool.clone());
        let models = ModelRepo::new(db.pool.clone());
        let runs = RunRepo::new(db.pool.clone());
        let run_events = RunEventRepo::new(db.pool.clone());
        purge_startup_history(&callback_logs, &run_events).await?;

        let react = ReactRepo::new(db.pool.clone());
        let api_tokens = ApiTokenRepo::new(db.pool.clone());
        let usage = UsageRepo::new(db.pool.clone());
        let elicitation = ElicitationRepo::new(db.pool.clone());
        let jobs = JobRepo::new(db.pool.clone());
        let artifact_repo = ArtifactRepo::new(db.pool.clone());
        let artifacts = Arc::new(ArtifactStore::new(
            paths.artifacts_dir.clone(),
            artifact_repo,
        )?);

        let facade = Arc::new(SessionFacade::new(
            adk.clone(),
            SessionMetaRepo::new(db.pool.clone()),
        ));
        reconcile_startup_state(&runs, &facade).await?;

        let orchestrator = RunOrchestrator::new(RunRepo::new(db.pool.clone()));
        let run_streams = RunStreamRegistry::with_event_repo(run_events.clone());
        let elicitation_broker = ElicitationBroker::new();
        let dynamic_elicitation = DynamicElicitationHandler::new(
            orchestrator.clone(),
            ElicitationRepo::new(db.pool.clone()),
            elicitation_broker.clone(),
            Some(run_streams.clone()),
        );
        let mcp_pool = Arc::new(McpPool::new(
            McpServerRepo::new(db.pool.clone()),
            dynamic_elicitation,
            paths.tmp_dir.clone(),
        ));
        reload_mcp_pool_on_startup(&mcp_pool).await;

        let filesystem_mcp = Arc::new(FilesystemMcpCoordinator::new(
            McpServerRepo::new(db.pool.clone()),
            paths.tmp_dir.clone(),
            mcp_pool.elicitation(),
        ));

        let skills = SkillRepo::new(db.pool.clone());
        let adk_skills = Arc::new(AdkSkillManager::new());

        let worktree_path_guard = worktree_path_guard_enabled(&settings).await?;

        let sub_agent_runs = SubAgentRunRepo::new(db.pool.clone());

        let adk_for_state = adk.clone();
        let harness = Arc::new(MacoHarness::new(
            adk,
            orchestrator,
            ReactRepo::new(db.pool.clone()),
            CallbackLogRepo::new(db.pool.clone()),
            UsageRepo::new(db.pool.clone()),
            ElicitationRepo::new(db.pool.clone()),
            policies,
            worktree_path_guard,
            mcp_pool.clone(),
            filesystem_mcp.clone(),
            elicitation_broker.clone(),
            run_streams,
            paths.tmp_dir.clone(),
            SessionMetaRepo::new(db.pool.clone()),
            artifacts.clone(),
            adk_skills.clone(),
            sub_agent_runs.clone(),
        ));

        sync_skills_on_startup(&skills, adk_skills.as_ref()).await;

        Ok(Self {
            runtime: RuntimeContext {
                bind_addr,
                auth_disabled: auth_disabled(),
                tmp_dir: paths.tmp_dir.clone(),
            },
            repos: RepoContext {
                meta,
                models,
                runs,
                run_events,
                react,
                api_tokens,
                usage,
                elicitation,
                jobs,
                mcp_servers,
                tool_policies: tool_policies_repo,
                settings,
                skills,
                sub_agent_runs,
            },
            agent: AgentContext {
                adk_skills,
                adk: adk_for_state,
                facade,
                harness,
                mcp_pool,
                filesystem_mcp,
            },
            storage: StorageContext { artifacts },
        })
    }
}

async fn seed_application_defaults(
    settings: &SettingsRepo,
    tool_policies: &ToolPolicyRepo,
    mcp_servers: &McpServerRepo,
    tmp_dir: &Path,
) -> maco_core::MacoResult<Vec<ToolPolicyRecord>> {
    seed_defaults(settings).await?;
    seed_tool_policies(tool_policies).await?;
    seed_default_filesystem_mcp(mcp_servers, tmp_dir).await?;
    tool_policies.list_enabled().await
}

async fn purge_startup_history(
    callback_logs: &CallbackLogRepo,
    run_events: &RunEventRepo,
) -> maco_core::MacoResult<()> {
    let purged = callback_logs.purge_older_than_days(30).await?;
    if purged > 0 {
        tracing::info!("purged {purged} callback log rows older than 30 days");
    }
    let purged_run_events = run_events.purge_older_than_days(30).await?;
    if purged_run_events > 0 {
        tracing::info!("purged {purged_run_events} run SSE event rows older than 30 days");
    }
    Ok(())
}

async fn reconcile_startup_state(
    runs: &RunRepo,
    facade: &SessionFacade,
) -> maco_core::MacoResult<()> {
    let stale = runs.fail_stale_active_runs("server restarted").await?;
    if stale > 0 {
        tracing::info!("marked {stale} stale active run(s) as failed after restart");
    }
    facade.reconcile().await?;
    Ok(())
}

async fn reload_mcp_pool_on_startup(mcp_pool: &McpPool) {
    if let Err(e) = mcp_pool.reload().await {
        tracing::warn!("mcp pool initial reload: {e}");
    }
}

async fn sync_skills_on_startup(skills: &SkillRepo, adk_skills: &AdkSkillManager) {
    if let Err(e) = skills_sync::sync_skills(skills, adk_skills, None).await {
        tracing::warn!("skill sync on startup: {e}");
    }
}
