use std::path::Path;

use maco_core::{adk_memory_url, adk_session_url, maco_db_url, MacoError, MacoResult};
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};

#[derive(Clone)]
pub struct MacoDb {
    pub pool: SqlitePool,
}

pub async fn init_pool(path: &Path) -> MacoResult<MacoDb> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| MacoError::database(format!("create dir: {e}")))?;
    }
    let url = maco_db_url(path);
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&url)
        .await
        .map_err(|e| MacoError::database(format!("connect: {e}")))?;
    sqlx::query("PRAGMA journal_mode = WAL")
        .execute(&pool)
        .await
        .map_err(|e| MacoError::database(format!("wal: {e}")))?;
    sqlx::migrate!("../../migrations")
        .run(&pool)
        .await
        .map_err(|e| MacoError::database(format!("migrate: {e}")))?;
    Ok(MacoDb { pool })
}

/// WAL checkpoint before backup (R24). Best-effort for maco.db.
pub async fn wal_checkpoint(path: &Path) -> MacoResult<()> {
    if !path.exists() {
        return Ok(());
    }
    let url = maco_db_url(path);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .map_err(|e| MacoError::database(format!("checkpoint connect: {e}")))?;
    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(&pool)
        .await
        .map_err(|e| MacoError::database(format!("checkpoint: {e}")))?;
    pool.close().await;
    Ok(())
}

/// WAL checkpoint for adk SQLite files (sessions.db / memory.db).
pub async fn wal_checkpoint_adk(path: &Path) -> MacoResult<()> {
    if !path.exists() {
        return Ok(());
    }
    let url = if path.file_name().and_then(|s| s.to_str()) == Some("memory.db") {
        adk_memory_url(path)
    } else {
        adk_session_url(path)
    };
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .map_err(|e| MacoError::database(format!("adk checkpoint connect: {e}")))?;
    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(&pool)
        .await
        .map_err(|e| MacoError::database(format!("adk checkpoint: {e}")))?;
    pool.close().await;
    Ok(())
}
