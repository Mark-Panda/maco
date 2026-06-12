//! ADK 遥测初始化：OTLP 导出或内存 `AdkSpanExporter`。

use std::sync::Arc;

use adk_telemetry::span_exporter::AdkSpanExporter;
use adk_telemetry::{init_with_adk_exporter, init_with_otlp, TelemetryError};

/// 遥测初始化结果（内存 span 导出器，供调试/后续 API 扩展）。
pub struct TelemetryInit {
    pub adk_exporter: Option<Arc<AdkSpanExporter>>,
}

/// 初始化全局 `tracing` 订阅器（仅应调用一次）。
pub fn init_maco_tracing() -> TelemetryInit {
    if let Ok(endpoint) = std::env::var("MACO_OTLP_ENDPOINT") {
        let endpoint = endpoint.trim();
        if !endpoint.is_empty() {
            match init_with_otlp("maco-server", endpoint) {
                Ok(()) => {
                    tracing::info!(otlp.endpoint = %endpoint, "ADK OTLP telemetry enabled");
                    return TelemetryInit { adk_exporter: None };
                }
                Err(TelemetryError::Init(msg)) => {
                    eprintln!("MACO_OTLP_ENDPOINT init failed ({msg}); falling back to ADK span exporter");
                }
            }
        }
    }

    match init_with_adk_exporter("maco-server") {
        Ok(exporter) => {
            tracing::info!("ADK in-memory span exporter enabled (set MACO_OTLP_ENDPOINT for OTLP)");
            TelemetryInit {
                adk_exporter: Some(exporter),
            }
        }
        Err(TelemetryError::Init(msg)) => {
            tracing_subscriber::fmt()
                .with_env_filter(
                    tracing_subscriber::EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("maco=info")),
                )
                .init();
            tracing::warn!("telemetry init fallback (plain fmt): {msg}");
            TelemetryInit { adk_exporter: None }
        }
    }
}
