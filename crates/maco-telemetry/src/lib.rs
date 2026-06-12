//! 可观测性：ADK tracing/OTLP + callback 结构化日志落库。

pub mod callback_logger;
pub mod tracing_init;

pub use callback_logger::MacoCallbackLogger;
pub use tracing_init::{init_maco_tracing, TelemetryInit};
