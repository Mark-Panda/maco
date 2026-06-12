//! 将 adk Agent 各阶段回调写入 `maco_callback_logs`，并将 LLM 请求/响应打印到控制台。

use std::sync::Arc;

use maco_db::CallbackLogRepo;
use uuid::Uuid;

/// 是否将 LLM 请求/响应打印到服务端控制台（默认开启；`MACO_MODEL_CONSOLE_LOG=off` 关闭）。
fn model_console_log_enabled() -> bool {
    match std::env::var("MACO_MODEL_CONSOLE_LOG") {
        Ok(v) => !(v == "off" || v == "0" || v.eq_ignore_ascii_case("false")),
        Err(_) => true,
    }
}

fn emit_model_console(phase: &str, session_id: &str, run_id: &str, body: &str) {
    if !model_console_log_enabled() {
        return;
    }
    tracing::info!(
        target: "maco::model",
        session_id = %session_id,
        run_id = %run_id,
        phase = %phase,
        "\n──────── LLM {phase} ────────\n{body}\n────────────────────────"
    );
}

/// 跳过流式中间 chunk（`partial=true` 且 `turn_complete=false`），与用量统计逻辑一致。
fn should_log_model_response(body: &str) -> bool {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(body) else {
        return true;
    };
    let partial = v.get("partial").and_then(|b| b.as_bool()).unwrap_or(false);
    let turn_complete = v
        .get("turn_complete")
        .and_then(|b| b.as_bool())
        .unwrap_or(false);
    !(partial && !turn_complete)
}

/// 绑定 session/run 的回调日志记录器。
pub struct MacoCallbackLogger {
    repo: CallbackLogRepo,
    session_id: String,
    run_id: String,
}

impl MacoCallbackLogger {
    pub fn new(repo: CallbackLogRepo, session_id: String, run_id: String) -> Arc<Self> {
        Arc::new(Self {
            repo,
            session_id,
            run_id,
        })
    }

    pub async fn log_phase(
        &self,
        callback_type: &str,
        session_id: &str,
        input: Option<&str>,
        output: Option<&str>,
    ) {
        if callback_type == "before_model" {
            if let Some(body) = input {
                emit_model_console("request", session_id, &self.run_id, body);
            }
        } else if callback_type == "after_model" {
            let Some(body) = output else {
                return;
            };
            if !should_log_model_response(body) {
                return;
            }
            emit_model_console("response", session_id, &self.run_id, body);
        }

        let span_id = Uuid::new_v4().to_string();
        if let Err(e) = self
            .repo
            .insert_started(
                session_id,
                &self.run_id,
                &span_id,
                callback_type,
                input,
                None,
            )
            .await
        {
            tracing::error!("callback log write failed: {e}");
            return;
        }
        if output.is_some() {
            let _ = self
                .repo
                .complete_span(&span_id, output, "completed", 0, None)
                .await;
        }
    }

    pub async fn log_tool_start(&self, tool_name: &str, input: &str) {
        let span_id = Uuid::new_v4().to_string();
        if let Err(e) = self
            .repo
            .insert_started(
                &self.session_id,
                &self.run_id,
                &span_id,
                "before_tool",
                Some(input),
                Some(tool_name),
            )
            .await
        {
            tracing::error!("callback log write failed: {e}");
        }
    }

    pub async fn log_tool_end(
        &self,
        tool_name: &str,
        output: &str,
        error_message: Option<&str>,
    ) {
        let span_id = Uuid::new_v4().to_string();
        let status = if error_message.is_some() {
            "failed"
        } else {
            "completed"
        };
        if let Err(e) = self
            .repo
            .insert_started(
                &self.session_id,
                &self.run_id,
                &span_id,
                "after_tool",
                None,
                Some(tool_name),
            )
            .await
        {
            tracing::error!("callback log write failed: {e}");
            return;
        }
        if let Err(e) = self
            .repo
            .complete_span(&span_id, Some(output), status, 0, error_message)
            .await
        {
            tracing::error!("callback log complete failed: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_partial_streaming_chunks() {
        let body = r#"{"partial":true,"turn_complete":false,"content":{"parts":[{"thinking":" a"}]}}"#;
        assert!(!should_log_model_response(body));
    }

    #[test]
    fn logs_final_response() {
        let body = r#"{"partial":false,"turn_complete":true,"finish_reason":"stop"}"#;
        assert!(should_log_model_response(body));
    }

    #[test]
    fn logs_partial_when_turn_complete() {
        let body = r#"{"partial":true,"turn_complete":true}"#;
        assert!(should_log_model_response(body));
    }
}
