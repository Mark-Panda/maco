//! 应用配置与数据目录路径（`~/.maco/config.toml` + `~/.maco/data/`）。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{MacoError, MacoResult};

/// adk 应用名（写入 sessions.db）。
pub const APP_NAME: &str = "maco";
/// 本机单用户模式下的固定 user_id。
pub const USER_ID: &str = "local";

/// 顶层配置，当前仅包含数据路径。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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
    /// Agent 临时工作区根目录（每会话子目录在其下）。
    pub tmp_dir: PathBuf,
}

impl Default for DataPaths {
    fn default() -> Self {
        let base = default_data_dir();
        Self {
            maco_db: base.join("maco.db"),
            sessions_db: base.join("sessions.db"),
            memory_db: base.join("memory.db"),
            artifacts_dir: base.join("artifacts"),
            tmp_dir: default_tmp_dir(),
        }
    }
}

/// 默认数据根目录 `~/.maco/data`。
pub fn default_data_dir() -> PathBuf {
    maco_home_dir().join("data")
}

/// 默认 Skill 扫描目录 `~/.maco/skills`。
pub fn default_skills_dir() -> PathBuf {
    maco_home_dir().join("skills")
}

/// maco 配置根目录 `~/.maco`。
pub fn maco_home_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".maco")
}

/// Agent 临时文件根目录 `~/.maco/tmp`。
pub fn default_tmp_dir() -> PathBuf {
    maco_home_dir().join("tmp")
}

/// 单会话工作区 `~/.maco/tmp/sessions/{session_id}`。
pub fn session_workspace_dir(tmp_root: &Path, session_id: &str) -> PathBuf {
    tmp_root.join("sessions").join(session_id)
}

/// 创建会话工作区目录并返回绝对路径。
pub fn ensure_session_workspace(tmp_root: &Path, session_id: &str) -> MacoResult<PathBuf> {
    let dir = session_workspace_dir(tmp_root, session_id);
    std::fs::create_dir_all(&dir)
        .map_err(|e| MacoError::config(format!("create session workspace: {e}")))?;
    Ok(dir)
}

/// 读取 `~/.maco/config.toml`；不存在则返回默认配置。
pub fn load_config() -> MacoResult<AppConfig> {
    let path = config_file_path();
    if !path.exists() {
        return Ok(AppConfig::default());
    }
    let raw = std::fs::read_to_string(&path)
        .map_err(|e| MacoError::config(format!("read {}: {e}", path.display())))?;
    let mut cfg: AppConfig =
        toml::from_str(&raw).map_err(|e| MacoError::config(format!("parse config: {e}")))?;
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
    paths.maco_db = expand_tilde_path(paths.maco_db);
    paths.sessions_db = expand_tilde_path(paths.sessions_db);
    paths.memory_db = expand_tilde_path(paths.memory_db);
    paths.artifacts_dir = expand_tilde_path(paths.artifacts_dir);
    paths.tmp_dir = expand_tilde_path(paths.tmp_dir);
    paths
}

/// 展开路径中的 `~` / `~/` 为当前用户主目录。
pub fn expand_tilde_path(path: PathBuf) -> PathBuf {
    let s = path.to_string_lossy();
    if let Some(home) = dirs::home_dir() {
        if s.as_ref() == "~" {
            return home;
        }
        if let Some(rest) = s.strip_prefix("~/") {
            return home.join(rest);
        }
    }
    path
}

fn validate_project_root_access(path: &Path) -> MacoResult<()> {
    if !path.exists() {
        return Err(MacoError::config(format!(
            "project_root does not exist: {}",
            path.display()
        )));
    }
    if !path.is_dir() {
        return Err(MacoError::config(format!(
            "project_root is not a directory: {}",
            path.display()
        )));
    }
    std::fs::read_dir(path).map_err(|e| {
        MacoError::config(format!(
            "project_root is not readable ({}): {e}",
            path.display()
        ))
    })?;
    let probe = path.join(".maco_write_probe");
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
    {
        Ok(handle) => drop(handle),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => {
            return Err(MacoError::config(format!(
                "project_root is not writable ({}): {e}",
                path.display()
            )));
        }
    }
    let _ = std::fs::remove_file(probe);
    Ok(())
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
    std::fs::create_dir_all(&paths.tmp_dir)
        .map_err(|e| MacoError::config(format!("create tmp dir: {e}")))?;
    std::fs::create_dir_all(paths.tmp_dir.join("sessions"))
        .map_err(|e| MacoError::config(format!("create tmp sessions dir: {e}")))?;
    std::fs::create_dir_all(default_skills_dir())
        .map_err(|e| MacoError::config(format!("create skills dir: {e}")))?;
    std::fs::create_dir_all(crate::maco_home_dir().join("worktrees"))
        .map_err(|e| MacoError::config(format!("create worktrees dir: {e}")))?;
    Ok(())
}

/// 解析并校验会话绑定的项目根目录（须为绝对路径，可含 `~`）。
pub fn resolve_project_root(raw: Option<&str>) -> MacoResult<Option<PathBuf>> {
    let Some(s) = raw.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    let path = expand_tilde_path(PathBuf::from(s));
    if !path.is_absolute() {
        return Err(MacoError::config(
            "project_root must be an absolute path (e.g. /Users/you/project or ~/code/app)",
        ));
    }
    validate_project_root_access(&path)?;
    Ok(Some(path))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_workspace_under_tmp() {
        let root = PathBuf::from("/home/u/.maco/tmp");
        let ws = session_workspace_dir(&root, "abc-123");
        assert_eq!(ws, PathBuf::from("/home/u/.maco/tmp/sessions/abc-123"));
    }

    #[test]
    fn resolve_project_root_rejects_relative() {
        assert!(resolve_project_root(Some("relative/path")).is_err());
    }
}
