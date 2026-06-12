//! maco 核心类型：配置路径、错误、Run/SSE 契约、模型密钥脱敏。

pub mod config;
pub mod error;
pub mod git_worktree;
pub mod model_config;
pub mod redact;
pub mod types;
pub mod workspace;

pub use config::{
    default_data_dir, default_skills_dir, default_tmp_dir, ensure_data_dirs, ensure_session_workspace,
    expand_tilde_path, load_config, maco_home_dir, resolve_project_root, session_workspace_dir,
    sqlite_url, adk_session_url, adk_memory_url, maco_db_url, AppConfig, DataPaths, APP_NAME, USER_ID,
};
pub use git_worktree::{
    bash_command_targets_main_repo, branch_name, current_branch, ensure_worktree,
    git_worktree_status, is_git_repository, probe_git_repository, remove_worktree,
    worktree_path_for_session, DEFAULT_GIT_BRANCH_PREFIX, GitRepoProbe, GitWorktreeInfo,
};
pub use workspace::{resolve_session_workspace, workspace_from_cached, SessionWorkspace};
pub use error::{MacoError, MacoResult};
pub use model_config::{
    api_key_from_config, api_key_preview, has_stored_api_key, merge_api_key, redact_config_for_api,
};
pub use redact::{basic_redact, prepare_log_payload, truncate_json};
pub use types::*;
