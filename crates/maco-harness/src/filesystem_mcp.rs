//! Per-run filesystem MCP 子进程：每轮 Run 独立 `npx @modelcontextprotocol/server-filesystem`，
//! 避免全局 `McpPool.reload()` 在多会话间争抢根目录。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use adk_core::Toolset;
use adk_tool::mcp::McpServerConfig;
use adk_tool::{ElicitationHandler, McpServerManager};
use maco_core::{MacoError, MacoResult};
use maco_db::{FILESYSTEM_MCP_NAME, McpServerRepo, filesystem_mcp_args};
use tokio::sync::Mutex;
use tracing::{info, warn};

use crate::elicitation::DynamicElicitationHandler;

/// 单轮 Run 持有的 filesystem MCP 子进程；Drop 时随 `McpServerManager` 释放。
pub struct SessionFilesystemMcp {
    manager: Arc<McpServerManager>,
}

impl SessionFilesystemMcp {
    /// 按允许根目录启动独立 filesystem MCP（不修改 DB、不重载全局池）。
    pub async fn spawn(
        mcp_servers: &McpServerRepo,
        tmp_dir: &Path,
        allowed_roots: &[String],
        elicitation: Arc<DynamicElicitationHandler>,
    ) -> MacoResult<Self> {
        let rec = mcp_servers
            .get_by_name(FILESYSTEM_MCP_NAME)
            .await?
            .ok_or_else(|| MacoError::config("filesystem MCP is not configured"))?;
        if rec.transport != "stdio" {
            return Err(MacoError::config(format!(
                "filesystem MCP transport is {transport}, expected stdio",
                transport = rec.transport
            )));
        }
        let command = rec
            .command
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or("npx");
        let args_json = filesystem_mcp_args(allowed_roots)?;
        let args: Vec<String> = serde_json::from_str(&args_json)
            .map_err(|e| MacoError::config(format!("invalid filesystem mcp args: {e}")))?;
        let mut env: HashMap<String, String> = serde_json::from_str(&rec.env)
            .map_err(|e| MacoError::config(format!("invalid filesystem mcp env: {e}")))?;
        merge_tmp_env(&mut env, tmp_dir);

        let mut configs = HashMap::new();
        configs.insert(
            FILESYSTEM_MCP_NAME.to_string(),
            McpServerConfig {
                command: command.to_string(),
                args,
                env,
                disabled: false,
                auto_approve: vec![],
                restart_policy: None,
            },
        );

        let manager = Arc::new(
            McpServerManager::new(configs)
                .with_elicitation_handler(elicitation as Arc<dyn ElicitationHandler>)
                .with_health_check_interval(Duration::from_secs(30)),
        );
        let start_results = manager.start_all().await;
        for (name, res) in &start_results {
            match res {
                Ok(()) => info!("session filesystem mcp started: {name}"),
                Err(e) => warn!("session filesystem mcp failed to start {name}: {e}"),
            }
        }

        Ok(Self { manager })
    }

    pub fn toolset(&self) -> Arc<dyn Toolset> {
        self.manager.clone() as Arc<dyn Toolset>
    }
}

fn merge_tmp_env(env: &mut HashMap<String, String>, tmp_dir: &Path) {
    let tmp = tmp_dir.to_string_lossy().to_string();
    env.entry("TMPDIR".into()).or_insert_with(|| tmp.clone());
    env.entry("TEMP".into()).or_insert_with(|| tmp.clone());
    env.entry("TMP".into()).or_insert(tmp);
    env.entry("MACO_TMP".into())
        .or_insert_with(|| tmp_dir.to_string_lossy().to_string());
}

/// 按会话缓存 filesystem MCP 子进程，避免 HITL / resume 重复 `npx` 冷启动。
type SessionFilesystemCache = HashMap<String, (Vec<String>, Arc<SessionFilesystemMcp>)>;

/// 按会话缓存 filesystem MCP 子进程，避免 HITL / resume 重复 `npx` 冷启动。
#[derive(Clone)]
pub struct FilesystemMcpCoordinator {
    mcp_servers: McpServerRepo,
    tmp_dir: PathBuf,
    elicitation: Arc<DynamicElicitationHandler>,
    session_cache: Arc<Mutex<SessionFilesystemCache>>,
}

impl FilesystemMcpCoordinator {
    pub fn new(
        mcp_servers: McpServerRepo,
        tmp_dir: PathBuf,
        elicitation: Arc<DynamicElicitationHandler>,
    ) -> Self {
        Self {
            mcp_servers,
            tmp_dir,
            elicitation,
            session_cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// 获取或创建会话级 filesystem MCP（根目录一致时复用子进程）。
    pub async fn acquire_for_session(
        &self,
        session_id: &str,
        allowed_roots: &[String],
    ) -> MacoResult<Arc<SessionFilesystemMcp>> {
        let mut cache = self.session_cache.lock().await;
        if let Some((roots, handle)) = cache.get(session_id) {
            if roots == allowed_roots {
                return Ok(handle.clone());
            }
            cache.remove(session_id);
        }
        let handle = Arc::new(
            SessionFilesystemMcp::spawn(
                &self.mcp_servers,
                &self.tmp_dir,
                allowed_roots,
                self.elicitation.clone(),
            )
            .await?,
        );
        cache.insert(
            session_id.to_string(),
            (allowed_roots.to_vec(), handle.clone()),
        );
        Ok(handle)
    }

    /// Run 正常结束且非 `awaiting_user` 时释放缓存与子进程。
    pub async fn release_session(&self, session_id: &str) {
        self.session_cache.lock().await.remove(session_id);
    }

    /// 中断 Run 等场景：立即释放会话 filesystem 子进程缓存。
    pub async fn force_end_session_scope(&self, session_id: &str) -> MacoResult<()> {
        self.release_session(session_id).await;
        Ok(())
    }
}
