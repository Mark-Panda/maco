use maco_core::{MacoError, MacoResult};
use sqlx::SqlitePool;

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct UsageRow {
    pub id: i64,
    pub session_id: Option<String>,
    pub run_id: Option<String>,
    pub model_id: Option<String>,
    pub model_name: String,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
    pub estimated_cost: Option<f64>,
    pub created_at: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct UsageSummaryItem {
    pub key: String,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
    pub estimated_cost: f64,
    pub request_count: i64,
}

#[derive(Clone)]
pub struct UsageRepo {
    pool: SqlitePool,
}

impl UsageRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn insert(
        &self,
        session_id: Option<&str>,
        run_id: Option<&str>,
        model_id: Option<&str>,
        model_name: &str,
        prompt_tokens: i64,
        completion_tokens: i64,
        estimated_cost: Option<f64>,
    ) -> MacoResult<()> {
        let total = prompt_tokens + completion_tokens;
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO maco_usage_stats
             (session_id, run_id, model_id, model_name, prompt_tokens, completion_tokens, total_tokens, estimated_cost, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(session_id)
        .bind(run_id)
        .bind(model_id)
        .bind(model_name)
        .bind(prompt_tokens)
        .bind(completion_tokens)
        .bind(total)
        .bind(estimated_cost)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|e| MacoError::database(e.to_string()))?;
        Ok(())
    }

    pub async fn summary(
        &self,
        from: Option<&str>,
        to: Option<&str>,
        group_by: &str,
    ) -> MacoResult<Vec<UsageSummaryItem>> {
        let key_expr = match group_by {
            "day" => "substr(created_at, 1, 10)",
            "session" => "COALESCE(session_id, 'unknown')",
            _ => "model_name",
        };
        let mut sql = format!(
            "SELECT {key_expr} AS key,
                    SUM(prompt_tokens) AS prompt_tokens,
                    SUM(completion_tokens) AS completion_tokens,
                    SUM(total_tokens) AS total_tokens,
                    COALESCE(SUM(estimated_cost), 0) AS estimated_cost,
                    COUNT(*) AS request_count
             FROM maco_usage_stats WHERE 1=1"
        );
        if from.is_some() {
            sql.push_str(" AND created_at >= ?");
        }
        if to.is_some() {
            sql.push_str(" AND created_at <= ?");
        }
        sql.push_str(&format!(" GROUP BY {key_expr} ORDER BY key DESC"));

        let mut q = sqlx::query_as::<_, (String, i64, i64, i64, f64, i64)>(&sql);
        if let Some(f) = from {
            q = q.bind(f);
        }
        if let Some(t) = to {
            q = q.bind(t);
        }
        let rows = q
            .fetch_all(&self.pool)
            .await
            .map_err(|e| MacoError::database(e.to_string()))?;

        Ok(rows
            .into_iter()
            .map(
                |(key, prompt_tokens, completion_tokens, total_tokens, estimated_cost, request_count)| {
                    UsageSummaryItem {
                        key,
                        prompt_tokens,
                        completion_tokens,
                        total_tokens,
                        estimated_cost,
                        request_count,
                    }
                },
            )
            .collect())
    }
}
