//! `maco_sub_agent_runs`：子 Agent spawn 审计。

use maco_core::MacoError;
use sqlx::SqlitePool;
use uuid::Uuid;

/// 子 Agent 执行审计记录。
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct SubAgentRunRecord {
    pub id: String,
    pub session_id: String,
    pub parent_run_id: String,
    pub task_key: String,
    pub worker_agent: String,
    pub tools_profile: String,
    pub status: String,
    pub instruction: String,
    pub summary: Option<String>,
    pub error: Option<String>,
    pub spawn_count: i64,
    pub model_id: Option<String>,
    pub usage_tokens: Option<i64>,
    pub started_at: String,
    pub finished_at: Option<String>,
}

const INSTRUCTION_MAX: usize = 4000;
const SUMMARY_MAX: usize = 8000;
const ERROR_MAX: usize = 2000;

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    format!("{}…", s.chars().take(max).collect::<String>())
}

/// 子 Agent Run 持久化。
#[derive(Clone)]
pub struct SubAgentRunRepo {
    pool: SqlitePool,
}

impl SubAgentRunRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// 创建 `running` 记录；`spawn_count` 为同 parent_run + task_key 的第几次 spawn。
    pub async fn start(
        &self,
        session_id: &str,
        parent_run_id: &str,
        task_key: &str,
        worker_agent: &str,
        tools_profile: &str,
        instruction: &str,
        model_id: Option<&str>,
    ) -> maco_core::MacoResult<SubAgentRunRecord> {
        let count_row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM maco_sub_agent_runs WHERE parent_run_id = ? AND task_key = ?",
        )
        .bind(parent_run_id)
        .bind(task_key)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| MacoError::database(e.to_string()))?;
        let spawn_count = count_row.0 + 1;
        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let instruction = truncate(instruction, INSTRUCTION_MAX);
        sqlx::query(
            "INSERT INTO maco_sub_agent_runs (
                id, session_id, parent_run_id, task_key, worker_agent, tools_profile,
                status, instruction, spawn_count, model_id, started_at
             ) VALUES (?, ?, ?, ?, ?, ?, 'running', ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(session_id)
        .bind(parent_run_id)
        .bind(task_key)
        .bind(worker_agent)
        .bind(tools_profile)
        .bind(&instruction)
        .bind(spawn_count)
        .bind(model_id)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(|e| MacoError::database(e.to_string()))?;
        Ok(SubAgentRunRecord {
            id,
            session_id: session_id.to_string(),
            parent_run_id: parent_run_id.to_string(),
            task_key: task_key.to_string(),
            worker_agent: worker_agent.to_string(),
            tools_profile: tools_profile.to_string(),
            status: "running".into(),
            instruction,
            summary: None,
            error: None,
            spawn_count,
            model_id: model_id.map(str::to_string),
            usage_tokens: None,
            started_at: now,
            finished_at: None,
        })
    }

    pub async fn finish(
        &self,
        id: &str,
        status: &str,
        summary: Option<&str>,
        error: Option<&str>,
        usage_tokens: Option<i64>,
    ) -> maco_core::MacoResult<()> {
        let now = chrono::Utc::now().to_rfc3339();
        let summary = summary.map(|s| truncate(s, SUMMARY_MAX));
        let error = error.map(|s| truncate(s, ERROR_MAX));
        sqlx::query(
            "UPDATE maco_sub_agent_runs SET status = ?, summary = ?, error = ?, usage_tokens = ?, finished_at = ? WHERE id = ?",
        )
        .bind(status)
        .bind(summary)
        .bind(error)
        .bind(usage_tokens)
        .bind(now)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| MacoError::database(e.to_string()))?;
        Ok(())
    }

    pub async fn get(&self, id: &str) -> maco_core::MacoResult<Option<SubAgentRunRecord>> {
        sqlx::query_as::<_, SubAgentRunRecord>(
            "SELECT id, session_id, parent_run_id, task_key, worker_agent, tools_profile,
                    status, instruction, summary, error, spawn_count, model_id, usage_tokens,
                    started_at, finished_at
             FROM maco_sub_agent_runs WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| MacoError::database(e.to_string()))
    }

    pub async fn list_for_session(
        &self,
        session_id: &str,
        task_key: Option<&str>,
        limit: u32,
    ) -> maco_core::MacoResult<Vec<SubAgentRunRecord>> {
        let limit = limit.clamp(1, 200) as i64;
        if let Some(tk) = task_key.filter(|s| !s.is_empty()) {
            return sqlx::query_as::<_, SubAgentRunRecord>(
                "SELECT id, session_id, parent_run_id, task_key, worker_agent, tools_profile,
                        status, instruction, summary, error, spawn_count, model_id, usage_tokens,
                        started_at, finished_at
                 FROM maco_sub_agent_runs
                 WHERE session_id = ? AND task_key = ?
                 ORDER BY started_at DESC
                 LIMIT ?",
            )
            .bind(session_id)
            .bind(tk)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| MacoError::database(e.to_string()));
        }
        sqlx::query_as::<_, SubAgentRunRecord>(
            "SELECT id, session_id, parent_run_id, task_key, worker_agent, tools_profile,
                    status, instruction, summary, error, spawn_count, model_id, usage_tokens,
                    started_at, finished_at
             FROM maco_sub_agent_runs
             WHERE session_id = ?
             ORDER BY started_at DESC
             LIMIT ?",
        )
        .bind(session_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| MacoError::database(e.to_string()))
    }

    pub async fn find_running(
        &self,
        parent_run_id: &str,
        task_key: &str,
    ) -> maco_core::MacoResult<Option<SubAgentRunRecord>> {
        sqlx::query_as::<_, SubAgentRunRecord>(
            "SELECT id, session_id, parent_run_id, task_key, worker_agent, tools_profile,
                    status, instruction, summary, error, spawn_count, model_id, usage_tokens,
                    started_at, finished_at
             FROM maco_sub_agent_runs
             WHERE parent_run_id = ? AND task_key = ? AND status = 'running'
             ORDER BY started_at DESC
             LIMIT 1",
        )
        .bind(parent_run_id)
        .bind(task_key)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| MacoError::database(e.to_string()))
    }
}
