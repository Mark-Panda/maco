//! 应用配置与数据目录路径（`~/.maco/config.toml` + `~/.maco/data/`）。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{MacoError, MacoResult};

/// adk 应用名（写入 sessions.db）。
pub const APP_NAME: &str = "maco";
/// 本机单用户模式下的固定 user_id。
pub const USER_ID: &str = "local";

/// 顶层配置，当前仅包含数据路径。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// 数据文件路径配置。
    #[serde(default)]
    pub data: DataPaths,
}

/// 四类持久化路径：业务库、adk session、adk memory、artifact 目录。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataPaths {
    /// 业务库 `maco.db` 路径。
    pub maco_db: PathBuf,
    /// adk 会话库 `sessions.db` 路径。
    pub sessions_db: PathBuf,
    /// adk 记忆库 `memory.db` 路径。
    pub memory_db: PathBuf,
    /// 上传附件根目录。
    pub artifacts_dir: PathBuf,
}

impl Default for DataPaths {
    fn default() -> Self {
        let base = default_data_dir();
        Self {
            maco_db: base.join("maco.db"),
            sessions_db: base.join("sessions.db"),
            memory_db: base.join("memory.db"),
            artifacts_dir: base.join("artifacts"),
        }
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self { data: DataPaths::default() }
    }
}

/// 默认数据根目录 `~/.maco/data`。
pub fn default_data_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".maco")
        .join("data")
}

/// 默认 Skill 扫描目录 `~/.maco/skills`。
pub fn default_skills_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".maco")
        .join("skills")
}

/// 读取 `~/.maco/config.toml`；不存在则返回默认配置。
pub fn load_config() -> MacoResult<AppConfig> {
    let path = config_file_path();
    if !path.exists() {
        return Ok(AppConfig::default());
    }
    let raw = std::fs::read_to_string(&path)
        .map_err(|e| MacoError::config(format!("read {}: {e}", path.display())))?;
    let mut cfg: AppConfig = toml::from_str(&raw)
        .map_err(|e| MacoError::config(format!("parse config: {e}")))?;
    cfg.data = expand_paths(cfg.data);
    Ok(cfg)
}

fn config_file_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".maco")
        .join("config.toml")
}

fn expand_paths(mut paths: DataPaths) -> DataPaths {
    paths.maco_db = expand_tilde(paths.maco_db);
    paths.sessions_db = expand_tilde(paths.sessions_db);
    paths.memory_db = expand_tilde(paths.memory_db);
    paths.artifacts_dir = expand_tilde(paths.artifacts_dir);
    paths
}

fn expand_tilde(path: PathBuf) -> PathBuf {
    let s = path.to_string_lossy();
    if let Some(rest) = s.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    path
}

/// 确保数据目录与 artifacts 子目录存在。
pub fn ensure_data_dirs(paths: &DataPaths) -> MacoResult<()> {
    let parent = paths
        .maco_db
        .parent()
        .ok_or_else(|| MacoError::config("invalid maco_db path"))?;
    std::fs::create_dir_all(parent)
        .map_err(|e| MacoError::config(format!("create data dir: {e}")))?;
    std::fs::create_dir_all(&paths.artifacts_dir)
        .map_err(|e| MacoError::config(format!("create artifacts dir: {e}")))?;
    Ok(())
}

/// sqlx 业务库连接 URL（`maco.db`）。
pub fn sqlite_url(path: &Path) -> String {
    format!("sqlite://{}", path.display())
}

/// adk-session 用 SQLite URL；绝对路径需 `sqlite:///` 前缀。
pub fn adk_session_url(path: &Path) -> String {
    if path.is_absolute() {
        format!("sqlite:///{}?mode=rwc", path.display())
    } else {
        format!("sqlite:{}?mode=rwc", path.display())
    }
}

/// adk-memory 用 SQLite URL（`SqliteConnectOptions::from_str`）。
pub fn adk_memory_url(path: &Path) -> String {
    if path.is_absolute() {
        format!("sqlite:///{}", path.display())
    } else {
        format!("sqlite:{}", path.display())
    }
}

/// `maco-db` 迁移与连接使用的 URL。
pub fn maco_db_url(path: &Path) -> String {
    format!("sqlite://{}?mode=rwc", path.display())
}
