use std::collections::HashMap;
use std::sync::Arc;

use maco_core::{
    MacoError, MacoResult, RUN_STATUS_AWAITING_USER, RUN_STATUS_CANCELLED, RUN_STATUS_COMPLETED,
    RUN_STATUS_FAILED, RUN_STATUS_PENDING, RUN_STATUS_RUNNING,
};
use maco_db::{RunRecord, RunRepo};
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct RunOrchestrator {
    runs: RunRepo,
    session_locks: Arc<Mutex<HashMap<String, Arc<Mutex<()>>>>>,
}

impl RunOrchestrator {
    pub fn new(runs: RunRepo) -> Self {
        Self {
            runs,
            session_locks: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    async fn lock_for(&self, session_id: &str) -> Arc<Mutex<()>> {
        let mut map = self.session_locks.lock().await;
        map.entry(session_id.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    pub async fn start_run(&self, session_id: &str) -> MacoResult<RunRecord> {
        let lock = self.lock_for(session_id).await;
        let _guard = lock.lock().await;
        if self.runs.has_running(session_id).await? {
            return Err(MacoError::run("session already has a running run"));
        }
        let pending = self.runs.create(session_id, RUN_STATUS_PENDING).await?;
        self.runs
            .update_status(&pending.id, RUN_STATUS_RUNNING, None)
            .await?;
        self.runs
            .get(&pending.id)
            .await?
            .ok_or_else(|| MacoError::run("run missing after create"))
    }

    pub async fn complete_run(&self, run_id: &str) -> MacoResult<()> {
        self.runs
            .update_status(run_id, RUN_STATUS_COMPLETED, None)
            .await
    }

    pub async fn fail_run(&self, run_id: &str, err: &str) -> MacoResult<()> {
        self.runs
            .update_status(run_id, RUN_STATUS_FAILED, Some(err))
            .await
    }

    pub async fn cancel_run(&self, run_id: &str) -> MacoResult<()> {
        self.runs
            .update_status(run_id, RUN_STATUS_CANCELLED, None)
            .await
    }

    pub async fn get_run(&self, run_id: &str) -> MacoResult<Option<RunRecord>> {
        self.runs.get(run_id).await
    }

    pub async fn next_seq(&self, run_id: &str) -> MacoResult<u64> {
        self.runs.bump_seq(run_id).await
    }

    pub async fn await_user(&self, run_id: &str, resume_context: &str) -> MacoResult<()> {
        self.runs.set_awaiting_user(run_id, resume_context).await
    }

    pub async fn continue_from_awaiting(&self, run_id: &str) -> MacoResult<()> {
        self.runs
            .update_status(run_id, RUN_STATUS_RUNNING, None)
            .await?;
        self.runs.clear_resume_context(run_id).await
    }

    pub async fn start_resumed_run(&self, session_id: &str, parent_run_id: &str) -> MacoResult<RunRecord> {
        let lock = self.lock_for(session_id).await;
        let _guard = lock.lock().await;
        if self.runs.has_running(session_id).await? {
            return Err(MacoError::run("session already has a running run"));
        }
        let parent = self
            .runs
            .get(parent_run_id)
            .await?
            .ok_or_else(|| MacoError::not_found("parent run"))?;
        if parent.status != RUN_STATUS_AWAITING_USER {
            return Err(MacoError::conflict("run is not awaiting user"));
        }
        let pending = self.runs.create(session_id, RUN_STATUS_PENDING).await?;
        self.runs
            .set_superseded_by(parent_run_id, &pending.id)
            .await?;
        self.runs
            .update_status(parent_run_id, RUN_STATUS_COMPLETED, None)
            .await?;
        self.runs
            .update_status(&pending.id, RUN_STATUS_RUNNING, None)
            .await?;
        self.runs
            .get(&pending.id)
            .await?
            .ok_or_else(|| MacoError::run("run missing after resume create"))
    }
}
