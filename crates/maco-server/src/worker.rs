//! 后台定时任务 worker：每 30 秒扫描到期 job 并执行。

use std::sync::Arc;
use std::time::Duration;

use maco_db::JobRepo;
use tracing::{info, warn};

/// 启动后台轮询协程（进程生命周期内持续运行）。
pub fn spawn_job_worker(jobs: JobRepo) {
    tokio::spawn(async move {
        let jobs = Arc::new(jobs);
        loop {
            if let Err(e) = tick(&jobs).await {
                warn!("job worker tick failed: {e}");
            }
            tokio::time::sleep(Duration::from_secs(30)).await;
        }
    });
}

/// 单次扫描：取出 `next_run_at <= now` 的 job 并依次执行。
async fn tick(jobs: &JobRepo) -> maco_core::MacoResult<()> {
    let now = chrono::Utc::now().to_rfc3339();
    let due = jobs.due_jobs(&now).await?;
    for job in due {
        info!("running job {} ({})", job.name, job.job_type);
        jobs.update_run_result(&job.id, "running", None, None, None)
            .await?;
        let (status, result, err, next) = run_job(&job).await;
        jobs.update_run_result(&job.id, &status, result.as_deref(), err.as_deref(), next.as_deref())
            .await?;
    }
    Ok(())
}

/// 供 HTTP「立即执行」调用的公开入口。
pub async fn run_job_public(
    job: &maco_db::JobRecord,
) -> (String, Option<String>, Option<String>, Option<String>) {
    run_job(job).await
}

/// 按 `job_type` 分发执行逻辑，返回 (status, result, error, next_run_at)。
async fn run_job(job: &maco_db::JobRecord) -> (String, Option<String>, Option<String>, Option<String>) {
    match job.job_type.as_str() {
        "ping" => (
            "completed".into(),
            Some(format!("pong at {}", chrono::Utc::now().to_rfc3339())),
            None,
            schedule_next(&job.schedule),
        ),
        "log" => {
            let msg = serde_json::from_str::<serde_json::Value>(&job.payload)
                .ok()
                .and_then(|v| v.get("message").and_then(|m| m.as_str().map(str::to_string)))
                .unwrap_or_else(|| job.payload.clone());
            info!("job log [{}]: {msg}", job.name);
            (
                "completed".into(),
                Some(msg),
                None,
                schedule_next(&job.schedule),
            )
        }
        other => (
            "failed".into(),
            None,
            Some(format!("unknown job_type: {other}")),
            None,
        ),
    }
}

/// 根据 schedule 字符串计算下次运行时间（`hourly` / `daily`）。
fn schedule_next(schedule: &Option<String>) -> Option<String> {
    let schedule = schedule.as_deref()?;
    if schedule == "hourly" {
        return Some((chrono::Utc::now() + chrono::Duration::hours(1)).to_rfc3339());
    }
    if schedule == "daily" {
        return Some((chrono::Utc::now() + chrono::Duration::days(1)).to_rfc3339());
    }
    None
}
