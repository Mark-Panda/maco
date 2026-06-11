use std::sync::Arc;

use maco_db::CallbackLogRepo;
use uuid::Uuid;

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
