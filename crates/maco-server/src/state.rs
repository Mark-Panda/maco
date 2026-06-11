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

#[derive(Clone)]
pub struct AppState {
    pub bind_addr: String,
    pub auth_disabled: bool,
    pub meta: SessionMetaRepo,
    pub models: ModelRepo,
    pub runs: RunRepo,
    pub react: ReactRepo,
    pub api_tokens: ApiTokenRepo,
    pub usage: UsageRepo,
    pub elicitation: ElicitationRepo,
    pub jobs: JobRepo,
    pub adk: Arc<AdkStorage>,
    pub facade: Arc<SessionFacade>,
    pub harness: Arc<MacoHarness>,
    pub mcp_pool: Arc<McpPool>,
    pub artifacts: Arc<ArtifactStore>,
}

impl AppState {
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
