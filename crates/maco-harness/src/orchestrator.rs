//! Run 生命周期编排：会话级互斥、状态迁移与 SSE 序号分配。

use std::collections::HashMap;
use std::sync::Arc;

use maco_core::{
    MacoError, MacoResult, RUN_STATUS_AWAITING_USER, RUN_STATUS_CANCELLED, RUN_STATUS_COMPLETED,
    RUN_STATUS_FAILED, RUN_STATUS_PENDING, RUN_STATUS_RUNNING,
};
use maco_db::{RunRecord, RunRepo};
use tokio::sync::Mutex;

/// 管理 `maco_runs` 表上的 Run 状态机；同一会话同时仅允许一个 running Run。
#[derive(Clone)]
pub struct RunOrchestrator {
    runs: RunRepo,
    session_locks: Arc<Mutex<HashMap<String, Arc<Mutex<()>>>>>,
}

impl RunOrchestrator {
    /// 绑定 Run 仓储。
    pub fn new(runs: RunRepo) -> Self {
        Self {
            runs,
            session_locks: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// 获取会话级互斥锁，防止并发启动多个 Run。
    async fn lock_for(&self, session_id: &str) -> Arc<Mutex<()>> {
        let mut map = self.session_locks.lock().await;
        map.entry(session_id.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    /// 创建并进入 `running` 状态；若会话已有运行中 Run 则返回冲突错误。
    pub async fn start_run(&self, session_id: &str) -> MacoResult<RunRecord> {
        let lock = self.lock_for(session_id).await;
        let _guard = lock.lock().await;
        if self.runs.has_active_run(session_id).await? {
            return Err(MacoError::run("session already has an active run"));
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

    /// 标记 Run 正常结束。
    pub async fn complete_run(&self, run_id: &str) -> MacoResult<()> {
        self.runs
            .update_status(run_id, RUN_STATUS_COMPLETED, None)
            .await
    }

    /// 标记 Run 失败并记录错误信息。
    pub async fn fail_run(&self, run_id: &str, err: &str) -> MacoResult<()> {
        self.runs
            .update_status(run_id, RUN_STATUS_FAILED, Some(err))
            .await
    }

    /// 标记 Run 被用户或系统取消。
    pub async fn cancel_run(&self, run_id: &str) -> MacoResult<()> {
        self.runs
            .update_status(run_id, RUN_STATUS_CANCELLED, None)
            .await
    }

    /// 按 ID 查询 Run 记录。
    pub async fn get_run(&self, run_id: &str) -> MacoResult<Option<RunRecord>> {
        self.runs.get(run_id).await
    }

    /// 递增并返回 SSE 事件序号（`last_seq`）。
    pub async fn next_seq(&self, run_id: &str) -> MacoResult<u64> {
        self.runs.bump_seq(run_id).await
    }

    /// 进入 `awaiting_user`，持久化 `resume_context` 供 HITL/Elicitation 续跑。
    pub async fn await_user(&self, run_id: &str, resume_context: &str) -> MacoResult<()> {
        self.runs.set_awaiting_user(run_id, resume_context).await
    }

    /// 从 `awaiting_user` 恢复为 `running` 并清空 `resume_context`。
    pub async fn continue_from_awaiting(&self, run_id: &str) -> MacoResult<()> {
        self.runs
            .update_status(run_id, RUN_STATUS_RUNNING, None)
            .await?;
        self.runs.clear_resume_context(run_id).await
    }

    /// HITL 续跑：结束父 Run，创建新的子 Run 并置为 `running`。
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
