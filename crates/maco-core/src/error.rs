use thiserror::Error;

pub type MacoResult<T> = Result<T, MacoError>;

#[derive(Debug, Error)]
pub enum MacoError {
    #[error("config: {0}")]
    Config(String),
    #[error("database: {0}")]
    Database(String),
    #[error("session: {0}")]
    Session(String),
    #[error("run: {0}")]
    Run(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("validation: {0}")]
    Validation(String),
    #[error("adk: {0}")]
    Adk(String),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl MacoError {
    pub fn config(msg: impl Into<String>) -> Self {
        Self::Config(msg.into())
    }

    pub fn database(msg: impl Into<String>) -> Self {
        Self::Database(msg.into())
    }

    pub fn session(msg: impl Into<String>) -> Self {
        Self::Session(msg.into())
    }

    pub fn run(msg: impl Into<String>) -> Self {
        Self::Run(msg.into())
    }

    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::NotFound(msg.into())
    }

    pub fn conflict(msg: impl Into<String>) -> Self {
        Self::Conflict(msg.into())
    }
}
