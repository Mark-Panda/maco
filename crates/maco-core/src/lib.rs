//! maco 核心类型：配置路径、错误、Run/SSE 契约、模型密钥脱敏。

pub mod config;
pub mod error;
pub mod model_config;
pub mod redact;
pub mod types;

pub use config::{load_config, ensure_data_dirs, default_data_dir, default_skills_dir, sqlite_url, adk_session_url, adk_memory_url, maco_db_url, AppConfig, DataPaths, APP_NAME, USER_ID};
pub use error::{MacoError, MacoResult};
pub use model_config::{
    api_key_from_config, api_key_preview, has_stored_api_key, merge_api_key, redact_config_for_api,
};
pub use redact::{basic_redact, prepare_log_payload, truncate_json};
pub use types::*;
