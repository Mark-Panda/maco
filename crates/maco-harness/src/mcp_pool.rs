//! MCP 连接管理（ADK 原生）：`adk_tool::McpServerManager`（stdio）+ `McpHttpClientBuilder`（SSE）。
//! maco 仅负责从 DB 加载配置并热重载。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use adk_core::Toolset;
use adk_tool::mcp::McpServerConfig;
use adk_tool::{ElicitationHandler, McpHttpClientBuilder, McpServerManager};
use maco_core::{MacoError, MacoResult};
use maco_db::{FILESYSTEM_MCP_NAME, McpServerRecord, McpServerRepo};
use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::elicitation::DynamicElicitationHandler;

/// MCP 服务池：从 DB 加载配置并维护 adk MCP 连接。
pub struct McpPool {
    repo: McpServerRepo,
    elicitation: Arc<DynamicElicitationHandler>,
    tmp_dir: PathBuf,
    inner: Arc<RwLock<McpPoolState>>,
}

struct McpPoolState {
    manager: Arc<McpServerManager>,
    http_toolsets: HashMap<String, Arc<dyn Toolset>>,
}

impl McpPool {
    pub fn new(
        repo: McpServerRepo,
        elicitation: Arc<DynamicElicitationHandler>,
        tmp_dir: PathBuf,
    ) -> Self {
        let manager = Arc::new(McpServerManager::new(HashMap::new()).with_elicitation_handler(
            elicitation.clone() as Arc<dyn ElicitationHandler>,
        ));
        Self {
            repo,
            elicitation,
            tmp_dir,
            inner: Arc::new(RwLock::new(McpPoolState {
                manager,
                http_toolsets: HashMap::new(),
            })),
        }
    }

    pub fn elicitation(&self) -> Arc<DynamicElicitationHandler> {
        self.elicitation.clone()
    }

    pub fn tmp_dir(&self) -> &Path {
        &self.tmp_dir
    }

    /// 从 DB 重载并启动全部已启用 MCP 服务。
    pub async fn reload(&self) -> MacoResult<()> {
        let records = self.repo.list_enabled().await?;
        let mut stdio_configs: HashMap<String, McpServerConfig> = HashMap::new();
        let mut sse_records: Vec<McpServerRecord> = Vec::new();

        for rec in records {
            if rec.name == FILESYSTEM_MCP_NAME {
                continue;
            }
            if rec.transport == "stdio" {
                if let Some(cfg) = record_to_stdio_config(&rec, &self.tmp_dir)? {
                    stdio_configs.insert(rec.name.clone(), cfg);
                }
            } else if rec.transport == "sse" {
                sse_records.push(rec);
            }
        }

        let manager = Arc::new(
            McpServerManager::new(stdio_configs)
                .with_elicitation_handler(self.elicitation.clone() as Arc<dyn ElicitationHandler>)
                .with_health_check_interval(Duration::from_secs(30)),
        );

        let start_results = manager.start_all().await;
        for (name, res) in &start_results {
            match res {
                Ok(()) => info!("mcp stdio server started: {name}"),
                Err(e) => warn!("mcp stdio server failed to start {name}: {e}"),
            }
        }

        let mut http_toolsets: HashMap<String, Arc<dyn Toolset>> = HashMap::new();
        for rec in sse_records {
            let url = rec
                .url
                .as_deref()
                .filter(|u| !u.trim().is_empty())
                .ok_or_else(|| MacoError::config(format!("sse mcp {} missing url", rec.name)))?;
            match McpHttpClientBuilder::new(url)
                .with_elicitation_handler(self.elicitation.clone() as Arc<dyn ElicitationHandler>)
                .timeout(Duration::from_secs(60))
                .connect_with_elicitation()
                .await
            {
                Ok(toolset) => {
                    info!("mcp sse server connected: {}", rec.name);
                    let arc: Arc<dyn Toolset> = Arc::new(toolset);
                    http_toolsets.insert(rec.name.clone(), arc);
                }
                Err(e) => warn!("mcp sse server {} connect failed: {e}", rec.name),
            }
        }

        let mut guard = self.inner.write().await;
        guard.manager = manager;
        guard.http_toolsets = http_toolsets;
        Ok(())
    }

    /// 获取全部可挂到 Agent 的 toolset（manager 聚合 stdio + 各 sse）。
    pub async fn toolsets(&self) -> Vec<Arc<dyn Toolset>> {
        let guard = self.inner.read().await;
        let mut out: Vec<Arc<dyn Toolset>> = vec![guard.manager.clone() as Arc<dyn Toolset>];
        for ts in guard.http_toolsets.values() {
            out.push(ts.clone());
        }
        out
    }

    /// 健康检查占位：确认池已初始化。
    pub async fn acquire(&self, _server_name: &str) -> MacoConnGuard {
        MacoConnGuard
    }

    /// 管理器状态摘要（health API）。
    pub async fn status_summary(&self) -> Vec<String> {
        let guard = self.inner.read().await;
        let mut names: Vec<String> = guard.http_toolsets.keys().cloned().collect();
        names.push("stdio_manager".into());
        names
    }
}

/// 兼容旧 API 的 RAII 守卫（无操作）。
pub struct MacoConnGuard;

impl Drop for MacoConnGuard {
    fn drop(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use maco_db::McpServerRecord;

    fn sample_stdio_record() -> McpServerRecord {
        McpServerRecord {
            id: "id".into(),
            name: "fs".into(),
            transport: "stdio".into(),
            command: Some("npx".into()),
            args: r#"["-y","@modelcontextprotocol/server-filesystem","/tmp"]"#.into(),
            url: None,
            env: r#"{"FOO":"bar"}"#.into(),
            enabled: 1,
            created_at: "".into(),
            updated_at: "".into(),
        }
    }

    #[test]
    fn stdio_config_from_record() {
        let tmp = std::env::temp_dir();
        let cfg = record_to_stdio_config(&sample_stdio_record(), &tmp)
            .expect("ok")
            .expect("some");
        assert_eq!(cfg.command, "npx");
        assert_eq!(cfg.args.len(), 3);
        assert_eq!(cfg.env.get("FOO").map(String::as_str), Some("bar"));
        assert_eq!(cfg.env.get("TMPDIR").map(String::as_str), Some(tmp.to_str().unwrap()));
    }

    #[test]
    fn sse_record_returns_none_for_stdio_config() {
        let mut rec = sample_stdio_record();
        rec.transport = "sse".into();
        assert!(record_to_stdio_config(&rec, std::path::Path::new("/tmp"))
            .expect("ok")
            .is_none());
    }
}

fn merge_tmp_env(
    env: &mut std::collections::HashMap<String, String>,
    tmp_dir: &Path,
) {
    let tmp = tmp_dir.to_string_lossy().to_string();
    env.entry("TMPDIR".into()).or_insert_with(|| tmp.clone());
    env.entry("TEMP".into()).or_insert_with(|| tmp.clone());
    env.entry("TMP".into()).or_insert(tmp);
    env.entry("MACO_TMP".into())
        .or_insert_with(|| tmp_dir.to_string_lossy().to_string());
}

fn record_to_stdio_config(rec: &McpServerRecord, tmp_dir: &Path) -> MacoResult<Option<McpServerConfig>> {
    if rec.transport != "stdio" {
        return Ok(None);
    }
    let command = rec
        .command
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| MacoError::config("stdio mcp server requires command"))?;
    let args: Vec<String> = serde_json::from_str(&rec.args)
        .map_err(|e| MacoError::config(format!("invalid mcp args json: {e}")))?;
    let mut env: std::collections::HashMap<String, String> = serde_json::from_str(&rec.env)
        .map_err(|e| MacoError::config(format!("invalid mcp env json: {e}")))?;
    merge_tmp_env(&mut env, tmp_dir);
    Ok(Some(McpServerConfig {
        command: command.to_string(),
        args,
        env,
        disabled: rec.enabled == 0,
        auto_approve: vec![],
        restart_policy: None,
    }))
}
