//! `maco_skills` 表：Skill 启用状态与元数据（正文仍来自磁盘）。

use maco_core::{MacoError, MacoResult};
use sqlx::SqlitePool;
use uuid::Uuid;

/// Skill 元数据与启用状态。
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct SkillRecord {
    /// 记录 ID。
    pub id: String,
    /// Skill 名称（唯一）。
    pub name: String,
    /// 描述。
    pub description: Option<String>,
    /// SKILL 文件路径。
    pub file_path: Option<String>,
    /// 是否启用（1/0）。
    pub enabled: i64,
    /// 创建时间。
    pub created_at: String,
    /// 更新时间。
    pub updated_at: String,
}

#[derive(Clone)]
pub struct SkillRepo {
    pool: SqlitePool,
}

impl SkillRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn list(&self) -> MacoResult<Vec<SkillRecord>> {
        sqlx::query_as::<_, SkillRecord>("SELECT * FROM maco_skills ORDER BY name ASC")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| MacoError::config(format!("list skills: {e}")))
    }

    pub async fn get_by_name(&self, name: &str) -> MacoResult<Option<SkillRecord>> {
        sqlx::query_as::<_, SkillRecord>("SELECT * FROM maco_skills WHERE name = ?")
            .bind(name)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| MacoError::config(format!("get skill: {e}")))
    }

    pub async fn upsert_from_scan(
        &self,
        name: &str,
        description: &str,
        file_path: &str,
    ) -> MacoResult<SkillRecord> {
        if self.get_by_name(name).await?.is_some() {
            let now = chrono::Utc::now().to_rfc3339();
            sqlx::query(
                "UPDATE maco_skills SET description = ?, file_path = ?, updated_at = ? WHERE name = ?",
            )
            .bind(description)
            .bind(file_path)
            .bind(&now)
            .bind(name)
            .execute(&self.pool)
            .await
            .map_err(|e| MacoError::config(format!("update skill: {e}")))?;
            return self
                .get_by_name(name)
                .await?
                .ok_or_else(|| MacoError::config("skill missing after update"));
        }

        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO maco_skills (id, name, description, content, file_path, enabled, created_at, updated_at)
             VALUES (?, ?, ?, NULL, ?, 1, ?, ?)",
        )
        .bind(&id)
        .bind(name)
        .bind(description)
        .bind(file_path)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(|e| MacoError::config(format!("insert skill: {e}")))?;

        self.get_by_name(name)
            .await?
            .ok_or_else(|| MacoError::config("skill missing after insert"))
    }

    pub async fn delete_not_in(&self, names: &[String]) -> MacoResult<u64> {
        if names.is_empty() {
            let result = sqlx::query("DELETE FROM maco_skills")
                .execute(&self.pool)
                .await
                .map_err(|e| MacoError::config(format!("delete skills: {e}")))?;
            return Ok(result.rows_affected());
        }
        let placeholders = std::iter::repeat_n("?", names.len())
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!("DELETE FROM maco_skills WHERE name NOT IN ({placeholders})");
        let mut query = sqlx::query(&sql);
        for name in names {
            query = query.bind(name);
        }
        let result = query
            .execute(&self.pool)
            .await
            .map_err(|e| MacoError::config(format!("delete stale skills: {e}")))?;
        Ok(result.rows_affected())
    }

    pub async fn set_enabled(&self, name: &str, enabled: bool) -> MacoResult<SkillRecord> {
        let now = chrono::Utc::now().to_rfc3339();
        let flag = if enabled { 1 } else { 0 };
        let result =
            sqlx::query("UPDATE maco_skills SET enabled = ?, updated_at = ? WHERE name = ?")
                .bind(flag)
                .bind(&now)
                .bind(name)
                .execute(&self.pool)
                .await
                .map_err(|e| MacoError::config(format!("set skill enabled: {e}")))?;
        if result.rows_affected() == 0 {
            return Err(MacoError::not_found("skill"));
        }
        self.get_by_name(name)
            .await?
            .ok_or_else(|| MacoError::not_found("skill"))
    }

    pub async fn delete_by_name(&self, name: &str) -> MacoResult<()> {
        let result = sqlx::query("DELETE FROM maco_skills WHERE name = ?")
            .bind(name)
            .execute(&self.pool)
            .await
            .map_err(|e| MacoError::config(format!("delete skill row: {e}")))?;
        if result.rows_affected() == 0 {
            return Err(MacoError::not_found("skill"));
        }
        Ok(())
    }
}
