use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{MacoError, MacoResult};

pub const APP_NAME: &str = "maco";
pub const USER_ID: &str = "local";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub data: DataPaths,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataPaths {
    pub maco_db: PathBuf,
    pub sessions_db: PathBuf,
    pub memory_db: PathBuf,
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

pub fn default_data_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".maco")
        .join("data")
}

pub fn default_skills_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".maco")
        .join("skills")
}

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

pub fn sqlite_url(path: &Path) -> String {
    format!("sqlite://{}", path.display())
}

/// adk-session uses sqlx connect; absolute paths need `sqlite:///` prefix.
pub fn adk_session_url(path: &Path) -> String {
    if path.is_absolute() {
        format!("sqlite:///{}?mode=rwc", path.display())
    } else {
        format!("sqlite:{}?mode=rwc", path.display())
    }
}

/// adk-memory uses SqliteConnectOptions::from_str with create_if_missing.
pub fn adk_memory_url(path: &Path) -> String {
    if path.is_absolute() {
        format!("sqlite:///{}", path.display())
    } else {
        format!("sqlite:{}", path.display())
    }
}

pub fn maco_db_url(path: &Path) -> String {
    format!("sqlite://{}?mode=rwc", path.display())
}
