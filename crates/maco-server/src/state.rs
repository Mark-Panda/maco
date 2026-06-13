//! 应用全局状态：数据库仓库、Harness、MCP 池与 Session 门面。

use std::sync::Arc;

use maco_db::{
    init_pool, seed_default_filesystem_mcp, seed_defaults, seed_tool_policies, worktree_path_guard_enabled,
    ApiTokenRepo, ArtifactRepo, CallbackLogRepo, ElicitationRepo, JobRepo, McpServerRepo, ModelRepo,
    ReactRepo, RunRepo, SessionMetaRepo, SettingsRepo, SkillRepo, SubAgentRunRepo, ToolPolicyRepo,
    UsageRepo,
};
use maco_governance::auth_disabled;
use maco_harness::{
    DynamicElicitationHandler, ElicitationBroker, FilesystemMcpCoordinator, MacoHarness, McpPool,
    RunOrchestrator, AdkSkillManager, RunStreamRegistry,
};
use maco_storage::{AdkStorage, ArtifactStore};

use crate::session_facade::SessionFacade;
use crate::skills_sync;

/// Axum `State` 注入的共享上下文（各 HTTP handler 通过 `State<AppState>` 访问）。
#[derive(Clone)]
pub struct AppState {
    /// HTTP 监听地址（健康检查回显用）。
    pub bind_addr: String,
    /// 是否关闭 Bearer 鉴权。
    pub auth_disabled: bool,
    /// 会话元数据仓库。
    pub meta: SessionMetaRepo,
    /// 模型配置仓库。
    pub models: ModelRepo,
    /// Run 状态仓库。
    pub runs: RunRepo,
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
    /// 附件存储。
    pub artifacts: Arc<ArtifactStore>,
    /// 子 Agent spawn 审计。
    pub sub_agent_runs: SubAgentRunRepo,
    /// Agent 临时目录根路径。
    pub tmp_dir: std::path::PathBuf,
}

impl AppState {
    /// 工作区变更后丢弃会话级 filesystem MCP 缓存，下次 Run 按新根目录启动子进程。
    pub async fn invalidate_session_filesystem_cache(&self, session_id: &str) {
        self.filesystem_mcp.release_session(session_id).await;
    }

    /// Agent Run 期间禁止重载 MCP 连接池。
    pub async fn reload_mcp_pool_guarded(&self) -> maco_core::MacoResult<()> {
        if self.harness.run_streams().has_active().await {
            return Err(maco_core::MacoError::conflict(
                "cannot reload MCP while an agent run is active",
            ));
        }
        self.mcp_pool.reload().await?;
        Ok(())
    }

    /// 初始化连接池、迁移、默认种子、adk 存储、Harness 与启动对账。
    pub async fn new(bind_addr: String, db_path: &std::path::Path, paths: &maco_core::DataPaths) -> maco_core::MacoResult<Self> {
        let db = init_pool(db_path).await?;
        let settings = SettingsRepo::new(db.pool.clone());
        seed_defaults(&settings).await?;
        let tool_policies_repo = ToolPolicyRepo::new(db.pool.clone());
        seed_tool_policies(&tool_policies_repo).await?;
        let policies = tool_policies_repo.list_enabled().await?;

        let callback_logs = CallbackLogRepo::new(db.pool.clone());
        let purged = callback_logs.purge_older_than_days(30).await?;
        if purged > 0 {
            tracing::info!("purged {purged} callback log rows older than 30 days");
        }

        let adk = Arc::new(AdkStorage::open(paths).await?);
        let meta = SessionMetaRepo::new(db.pool.clone());
        let models = ModelRepo::new(db.pool.clone());
        let runs = RunRepo::new(db.pool.clone());
        let stale = runs.fail_stale_active_runs("server restarted").await?;
        if stale > 0 {
            tracing::info!("marked {stale} stale active run(s) as failed after restart");
        }

        let react = ReactRepo::new(db.pool.clone());
        let api_tokens = ApiTokenRepo::new(db.pool.clone());
        let usage = UsageRepo::new(db.pool.clone());
        let elicitation = ElicitationRepo::new(db.pool.clone());
        let jobs = JobRepo::new(db.pool.clone());
        let mcp_servers = McpServerRepo::new(db.pool.clone());
        seed_default_filesystem_mcp(&mcp_servers, &paths.tmp_dir).await?;
        let artifact_repo = ArtifactRepo::new(db.pool.clone());
        let artifacts = Arc::new(ArtifactStore::new(
            paths.artifacts_dir.clone(),
            artifact_repo,
        )?);

        let facade = Arc::new(SessionFacade::new(adk.clone(), SessionMetaRepo::new(db.pool.clone())));
        facade.reconcile().await?;

        let orchestrator = RunOrchestrator::new(RunRepo::new(db.pool.clone()));
        let run_streams = RunStreamRegistry::new();
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
        if let Err(e) = mcp_pool.reload().await {
            tracing::warn!("mcp pool initial reload: {e}");
        }

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

        if let Err(e) = skills_sync::sync_skills(&skills, adk_skills.as_ref(), None).await {
            tracing::warn!("skill sync on startup: {e}");
        }

        Ok(Self {
            bind_addr,
            auth_disabled: auth_disabled(),
            meta,
            models,
            runs,
            react,
            api_tokens,
            usage,
            elicitation,
            jobs,
            mcp_servers,
            tool_policies: tool_policies_repo,
            settings,
            skills,
            adk_skills,
            adk: adk_for_state,
            facade,
            harness,
            mcp_pool,
            filesystem_mcp,
            artifacts,
            sub_agent_runs,
            tmp_dir: paths.tmp_dir.clone(),
        })
    }
}
