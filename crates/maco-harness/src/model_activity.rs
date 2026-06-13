//! 模型调用期间的 SSE 活动提示与心跳（长耗时 LLM 等待时前端仍有反馈）。

use std::sync::Arc;
use std::time::Duration;

use adk_core::{AfterModelCallback, BeforeModelCallback, BeforeModelResult};
use maco_core::SseEnvelope;
use maco_telemetry::MacoCallbackLogger;
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;

use crate::orchestrator::RunOrchestrator;
use crate::run_stream::RunStreamRegistry;

/// 单次 Run 的模型活动 SSE 状态（含可取消心跳任务）。
#[derive(Clone)]
pub struct ModelActivityState {
    pub session_id: String,
    pub run_id: String,
    pub sse_tx: mpsc::Sender<SseEnvelope>,
    pub streams: RunStreamRegistry,
    pub orchestrator: RunOrchestrator,
    heartbeat: Arc<Mutex<Option<JoinHandle<()>>>>,
}

impl ModelActivityState {
    pub fn new(
        session_id: String,
        run_id: String,
        sse_tx: mpsc::Sender<SseEnvelope>,
        streams: RunStreamRegistry,
        orchestrator: RunOrchestrator,
    ) -> Self {
        Self {
            session_id,
            run_id,
            sse_tx,
            streams,
            orchestrator,
            heartbeat: Arc::new(Mutex::new(None)),
        }
    }

    async fn publish_activity(&self, message: &str, elapsed_secs: Option<u64>) {
        if let Ok(seq) = self.orchestrator.next_seq(&self.run_id).await {
            let env = SseEnvelope {
                event_type: "agent_activity".into(),
                run_id: self.run_id.clone(),
                seq,
                payload: serde_json::json!({
                    "message": message,
                    "phase": "model",
                    "elapsed_secs": elapsed_secs,
                }),
            };
            let _ = self.sse_tx.send(env.clone()).await;
            self.streams.publish(&self.session_id, env).await;
        }
    }

    async fn start_heartbeat(&self) {
        self.stop_heartbeat().await;
        self.publish_activity("正在等待模型响应…", None).await;
        let state = self.clone();
        let handle = tokio::spawn(async move {
            let mut elapsed = 0u64;
            loop {
                tokio::time::sleep(Duration::from_secs(15)).await;
                elapsed += 15;
                state
                    .publish_activity(
                        &format!("正在等待模型响应…（已等待 {elapsed} 秒）"),
                        Some(elapsed),
                    )
                    .await;
            }
        });
        *self.heartbeat.lock().await = Some(handle);
    }

    async fn stop_heartbeat(&self) {
        if let Some(handle) = self.heartbeat.lock().await.take() {
            handle.abort();
        }
    }
}

/// `before_model`：记录请求并启动活动心跳。
pub fn before_model_with_activity(
    logger: Arc<MacoCallbackLogger>,
    activity: Arc<ModelActivityState>,
) -> BeforeModelCallback {
    Box::new(move |ctx, request| {
        let logger = Arc::clone(&logger);
        let activity = Arc::clone(&activity);
        Box::pin(async move {
            let input = maco_governance::prepare_log_payload(
                &serde_json::to_value(&request.contents).unwrap_or_default(),
            );
            logger
                .log_phase("before_model", ctx.session_id(), Some(&input), None)
                .await;
            activity.start_heartbeat().await;
            Ok(BeforeModelResult::Continue(request))
        })
    })
}

/// `after_model`：停止心跳并记录响应。
pub fn after_model_with_activity(
    logger: Arc<MacoCallbackLogger>,
    usage: Option<crate::usage::SharedUsageContext>,
    activity: Arc<ModelActivityState>,
) -> AfterModelCallback {
    Box::new(move |ctx, response| {
        let logger = Arc::clone(&logger);
        let usage = usage.clone();
        let activity = Arc::clone(&activity);
        let resp = response.clone();
        Box::pin(async move {
            activity.stop_heartbeat().await;
            let output = maco_governance::prepare_log_payload(
                &serde_json::to_value(&resp).unwrap_or_default(),
            );
            logger
                .log_phase("after_model", ctx.session_id(), None, Some(&output))
                .await;
            if let Some(u) = usage {
                u.record_if_final(&resp).await;
            }
            Ok(Some(response))
        })
    })
}
