//! 全项目统一错误类型与便捷构造方法。

use thiserror::Error;

/// `MacoError` 的 Result 别名。
pub type MacoResult<T> = Result<T, MacoError>;

/// maco 分层错误：配置、数据库、Session、Run、adk 等。
#[derive(Debug, Error)]
pub enum MacoError {
    /// 配置或请求参数错误。
    #[error("config: {0}")]
    Config(String),
    /// 数据库/SQL 错误。
    #[error("database: {0}")]
    Database(String),
    /// 会话层业务错误。
    #[error("session: {0}")]
    Session(String),
    /// Run 状态机错误。
    #[error("run: {0}")]
    Run(String),
    /// 资源不存在（HTTP 404）。
    #[error("not found: {0}")]
    NotFound(String),
    /// 状态冲突（HTTP 409，如乐观锁）。
    #[error("conflict: {0}")]
    Conflict(String),
    /// 输入校验失败。
    #[error("validation: {0}")]
    Validation(String),
    /// adk 框架返回的错误。
    #[error("adk: {0}")]
    Adk(String),
    /// 未分类错误。
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl MacoError {
    /// 配置/参数类错误。
    pub fn config(msg: impl Into<String>) -> Self {
        Self::Config(msg.into())
    }

    /// SQLite / 仓储层错误。
    pub fn database(msg: impl Into<String>) -> Self {
        Self::Database(msg.into())
    }

    /// Session 元数据或 adk Session 错误。
    pub fn session(msg: impl Into<String>) -> Self {
        Self::Session(msg.into())
    }

    /// Run 状态机或编排错误。
    pub fn run(msg: impl Into<String>) -> Self {
        Self::Run(msg.into())
    }

    /// 资源不存在（HTTP 404）。
    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::NotFound(msg.into())
    }

    /// 并发冲突或状态不匹配（HTTP 409）。
    pub fn conflict(msg: impl Into<String>) -> Self {
        Self::Conflict(msg.into())
    }
}
