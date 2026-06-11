use maco_core::{MacoError, MacoResult};
use sqlx::SqlitePool;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct ApiTokenRecord {
    pub id: String,
    pub name: String,
    pub token_hash: String,
    pub scopes: String,
    pub expires_at: Option<String>,
    pub last_used_at: Option<String>,
    pub enabled: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ApiTokenListItem {
    pub id: String,
    pub name: String,
    pub scopes: Vec<String>,
    pub expires_at: Option<String>,
    pub last_used_at: Option<String>,
    pub enabled: bool,
    pub created_at: String,
}

#[derive(Clone)]
pub struct ApiTokenRepo {
    pool: SqlitePool,
}

impl ApiTokenRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn count_enabled(&self) -> MacoResult<i64> {
        let row: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM maco_api_tokens WHERE enabled = 1")
                .fetch_one(&self.pool)
                .await
                .map_err(|e| MacoError::database(e.to_string()))?;
        Ok(row.0)
    }

    pub async fn insert(
        &self,
        name: &str,
        token_hash: &str,
        scopes: &str,
        expires_at: Option<&str>,
    ) -> MacoResult<ApiTokenRecord> {
        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO maco_api_tokens (id, name, token_hash, scopes, expires_at, enabled, created_at)
             VALUES (?, ?, ?, ?, ?, 1, ?)",
        )
        .bind(&id)
        .bind(name)
        .bind(token_hash)
        .bind(scopes)
        .bind(expires_at)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(|e| MacoError::database(e.to_string()))?;
        Ok(ApiTokenRecord {
            id,
            name: name.to_string(),
            token_hash: token_hash.to_string(),
            scopes: scopes.to_string(),
            expires_at: expires_at.map(str::to_string),
            last_used_at: None,
            enabled: 1,
            created_at: now,
        })
    }

    pub async fn list(&self) -> MacoResult<Vec<ApiTokenListItem>> {
        let rows = sqlx::query_as::<_, ApiTokenRecord>(
            "SELECT id, name, token_hash, scopes, expires_at, last_used_at, enabled, created_at
             FROM maco_api_tokens ORDER BY created_at DESC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| MacoError::database(e.to_string()))?;
        Ok(rows.into_iter().map(Self::to_list_item).collect())
    }

    pub async fn find_by_hash(&self, token_hash: &str) -> MacoResult<Option<ApiTokenRecord>> {
        sqlx::query_as::<_, ApiTokenRecord>(
            "SELECT id, name, token_hash, scopes, expires_at, last_used_at, enabled, created_at
             FROM maco_api_tokens WHERE token_hash = ? AND enabled = 1",
        )
        .bind(token_hash)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| MacoError::database(e.to_string()))
    }

    pub async fn delete(&self, id: &str) -> MacoResult<bool> {
        let result = sqlx::query("DELETE FROM maco_api_tokens WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| MacoError::database(e.to_string()))?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn touch_last_used(&self, id: &str) -> MacoResult<()> {
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query("UPDATE maco_api_tokens SET last_used_at = ? WHERE id = ?")
            .bind(now)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| MacoError::database(e.to_string()))?;
        Ok(())
    }

    fn to_list_item(row: ApiTokenRecord) -> ApiTokenListItem {
        let scopes = serde_json::from_str(&row.scopes).unwrap_or_else(|_| vec!["*".into()]);
        ApiTokenListItem {
            id: row.id,
            name: row.name,
            scopes,
            expires_at: row.expires_at,
            last_used_at: row.last_used_at,
            enabled: row.enabled != 0,
            created_at: row.created_at,
        }
    }
}
