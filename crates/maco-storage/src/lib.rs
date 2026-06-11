pub mod artifacts;
pub mod memory_admin;

use std::sync::Arc;

use adk_memory::{MemoryService, MemoryServiceAdapter, SqliteMemoryService};
use adk_session::{SessionService, SqliteSessionService};
use maco_core::{adk_memory_url, adk_session_url, DataPaths, MacoError, MacoResult, APP_NAME, USER_ID};
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::SqlitePool;
use std::str::FromStr;

pub use artifacts::ArtifactStore;

pub struct AdkStorage {
    pub session: Arc<dyn SessionService>,
    pub memory: Arc<dyn MemoryService>,
    pub memory_adapter: Arc<dyn adk_core::Memory>,
    memory_pool: SqlitePool,
}

impl AdkStorage {
    pub async fn open(paths: &DataPaths) -> MacoResult<Self> {
        let session_url = adk_session_url(&paths.sessions_db);
        let memory_url = adk_memory_url(&paths.memory_db);

        let session_svc = SqliteSessionService::new(&session_url)
            .await
            .map_err(|e| MacoError::Adk(e.to_string()))?;
        session_svc
            .migrate()
            .await
            .map_err(|e| MacoError::Adk(e.to_string()))?;

        let memory_options = SqliteConnectOptions::from_str(&memory_url)
            .map_err(|e| MacoError::Adk(e.to_string()))?
            .create_if_missing(true);
        let memory_pool = SqlitePool::connect_with(memory_options)
            .await
            .map_err(|e| MacoError::Adk(e.to_string()))?;
        let memory_svc = SqliteMemoryService::from_pool(memory_pool.clone());
        memory_svc
            .migrate()
            .await
            .map_err(|e| MacoError::Adk(e.to_string()))?;

        let session: Arc<dyn SessionService> = Arc::new(session_svc);
        let memory: Arc<dyn MemoryService> = Arc::new(memory_svc);
        let memory_adapter = Arc::new(MemoryServiceAdapter::new(
            memory.clone(),
            APP_NAME,
            USER_ID,
        )) as Arc<dyn adk_core::Memory>;

        Ok(Self {
            session,
            memory,
            memory_adapter,
            memory_pool,
        })
    }

    pub fn memory_pool(&self) -> &SqlitePool {
        &self.memory_pool
    }

    pub fn session_service(&self) -> Arc<dyn SessionService> {
        self.session.clone()
    }

    pub fn memory_service(&self) -> Arc<dyn adk_core::Memory> {
        self.memory_adapter.clone()
    }
}
