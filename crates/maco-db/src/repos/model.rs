//! `maco_models` 表：LLM 提供商配置与默认模型。

use maco_core::{MacoError, MacoResult};
use sqlx::SqlitePool;
use uuid::Uuid;

/// 一条模型配置（`config` 为 JSON，可含内联 `api_key`）。
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize, serde::Deserialize)]
pub struct ModelRecord {
    /// 配置 ID。
    pub id: String,
    /// 显示名称。
    pub name: String,
    /// 提供商：`openai` / `anthropic` / `gemini` / `openrouter`。
    pub provider: String,
    /// 上游模型标识。
    pub model_id: String,
    /// 自定义 API Base URL。
    pub base_url: Option<String>,
    /// 环境变量名（API Key 兜底）。
    pub api_key_env: String,
    /// 是否默认（SQLite 0/1）。
    pub is_default: i64,
    /// 是否启用（SQLite 0/1）。
    pub enabled: i64,
    /// JSON 扩展配置（可含内联 api_key、单价等）。
    pub config: String,
    /// 创建时间。
    pub created_at: String,
    /// 最后更新时间。
    pub updated_at: String,
}

/// 模型 CRUD 与默认模型管理。
#[derive(Clone)]
pub struct ModelRepo {
    pool: SqlitePool,
}

impl ModelRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn list(&self) -> MacoResult<Vec<ModelRecord>> {
        sqlx::query_as::<_, ModelRecord>("SELECT * FROM maco_models ORDER BY name")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| MacoError::database(e.to_string()))
    }

    pub async fn get(&self, id: &str) -> MacoResult<Option<ModelRecord>> {
        sqlx::query_as::<_, ModelRecord>("SELECT * FROM maco_models WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| MacoError::database(e.to_string()))
    }

    pub async fn get_default(&self) -> MacoResult<Option<ModelRecord>> {
        sqlx::query_as::<_, ModelRecord>(
            "SELECT * FROM maco_models WHERE is_default = 1 AND enabled = 1 LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| MacoError::database(e.to_string()))
    }

    pub async fn insert(&self, rec: &ModelRecord) -> MacoResult<()> {
        sqlx::query(
            "INSERT INTO maco_models (id, name, provider, model_id, base_url, api_key_env, is_default, enabled, config, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&rec.id)
        .bind(&rec.name)
        .bind(&rec.provider)
        .bind(&rec.model_id)
        .bind(&rec.base_url)
        .bind(&rec.api_key_env)
        .bind(rec.is_default)
        .bind(rec.enabled)
        .bind(&rec.config)
        .bind(&rec.created_at)
        .bind(&rec.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|e| MacoError::database(e.to_string()))?;
        Ok(())
    }

    pub async fn clear_default_except(&self, except_id: &str) -> MacoResult<()> {
        sqlx::query("UPDATE maco_models SET is_default = 0 WHERE id != ?")
            .bind(except_id)
            .execute(&self.pool)
            .await
            .map_err(|e| MacoError::database(e.to_string()))?;
        Ok(())
    }

    pub async fn upsert(&self, rec: &ModelRecord) -> MacoResult<()> {
        if rec.is_default == 1 {
            self.clear_default_except(&rec.id).await?;
        }
        sqlx::query(
            "INSERT INTO maco_models (id, name, provider, model_id, base_url, api_key_env, is_default, enabled, config, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
               name=excluded.name, provider=excluded.provider, model_id=excluded.model_id,
               base_url=excluded.base_url, api_key_env=excluded.api_key_env,
               is_default=excluded.is_default, enabled=excluded.enabled,
               config=excluded.config, updated_at=excluded.updated_at",
        )
        .bind(&rec.id)
        .bind(&rec.name)
        .bind(&rec.provider)
        .bind(&rec.model_id)
        .bind(&rec.base_url)
        .bind(&rec.api_key_env)
        .bind(rec.is_default)
        .bind(rec.enabled)
        .bind(&rec.config)
        .bind(&rec.created_at)
        .bind(&rec.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|e| MacoError::database(e.to_string()))?;
        Ok(())
    }

    pub async fn delete(&self, id: &str) -> MacoResult<()> {
        sqlx::query("DELETE FROM maco_models WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| MacoError::database(e.to_string()))?;
        Ok(())
    }

    pub fn new_id() -> String {
        Uuid::new_v4().to_string()
    }
}
