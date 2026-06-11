use std::sync::Arc;
use std::time::Duration;

use maco_db::JobRepo;
use tracing::{info, warn};

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

pub async fn run_job_public(
    job: &maco_db::JobRecord,
) -> (String, Option<String>, Option<String>, Option<String>) {
    run_job(job).await
}

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
