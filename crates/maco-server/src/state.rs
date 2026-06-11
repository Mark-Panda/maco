//! 应用全局状态：数据库仓库、Harness、MCP 池与 Session 门面。

use std::sync::Arc;

use maco_db::{
    init_pool, seed_defaults, seed_tool_policies, ApiTokenRepo, ArtifactRepo, CallbackLogRepo,
    ElicitationRepo, JobRepo, ModelRepo, ReactRepo, RunRepo, SessionMetaRepo, SettingsRepo,
    ToolPolicyRepo, UsageRepo,
};
use maco_governance::auth_disabled;
use maco_harness::{MacoHarness, McpPool, RunOrchestrator};
use maco_storage::{AdkStorage, ArtifactStore};

use crate::session_facade::SessionFacade;

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
    /// adk Session/Memory 存储。
    pub adk: Arc<AdkStorage>,
    /// Session + Memory 门面。
    pub facade: Arc<SessionFacade>,
    /// Agent 编排 Harness。
    pub harness: Arc<MacoHarness>,
    /// MCP 连接池。
    pub mcp_pool: Arc<McpPool>,
    /// 附件存储。
    pub artifacts: Arc<ArtifactStore>,
}

impl AppState {
    /// 初始化连接池、迁移、默认种子、adk 存储、Harness 与启动对账。
    pub async fn new(bind_addr: String, db_path: &std::path::Path, paths: &maco_core::DataPaths) -> maco_core::MacoResult<Self> {
        let db = init_pool(db_path).await?;
        let settings = SettingsRepo::new(db.pool.clone());
        seed_defaults(&settings).await?;
        let tool_policies = ToolPolicyRepo::new(db.pool.clone());
        seed_tool_policies(&tool_policies).await?;
        let policies = tool_policies.list_enabled().await?;

        let callback_logs = CallbackLogRepo::new(db.pool.clone());
        let purged = callback_logs.purge_older_than_days(30).await?;
        if purged > 0 {
            tracing::info!("purged {purged} callback log rows older than 30 days");
        }

        let adk = Arc::new(AdkStorage::open(paths).await?);
        let meta = SessionMetaRepo::new(db.pool.clone());
        let models = ModelRepo::new(db.pool.clone());
        let runs = RunRepo::new(db.pool.clone());
        let react = ReactRepo::new(db.pool.clone());
        let api_tokens = ApiTokenRepo::new(db.pool.clone());
        let usage = UsageRepo::new(db.pool.clone());
        let elicitation = ElicitationRepo::new(db.pool.clone());
        let jobs = JobRepo::new(db.pool.clone());
        let artifact_repo = ArtifactRepo::new(db.pool.clone());
        let artifacts = Arc::new(ArtifactStore::new(paths.artifacts_dir.clone(), artifact_repo));

        let facade = Arc::new(SessionFacade::new(adk.clone(), SessionMetaRepo::new(db.pool.clone())));
        facade.reconcile().await?;

        let orchestrator = RunOrchestrator::new(RunRepo::new(db.pool.clone()));
        let adk_for_state = adk.clone();
        let harness = Arc::new(MacoHarness::new(
            adk,
            orchestrator,
            ReactRepo::new(db.pool.clone()),
            CallbackLogRepo::new(db.pool.clone()),
            UsageRepo::new(db.pool.clone()),
            ElicitationRepo::new(db.pool.clone()),
            policies,
        ));

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
            adk: adk_for_state,
            facade,
            harness,
            mcp_pool: Arc::new(McpPool::new()),
            artifacts,
        })
    }
}
