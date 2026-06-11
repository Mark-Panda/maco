//! MCP Elicitation 人机交互：持久化请求、SSE 推送、等待用户响应后恢复 Run。

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use adk_tool::ElicitationHandler;
use maco_core::{MacoError, MacoResult, ResumeContext, SseEnvelope, RUN_STATUS_AWAITING_USER};
use maco_db::ElicitationRepo;
use rmcp::model::{
    CreateElicitationResult, ElicitationAction, ElicitationSchema,
};
use serde_json::Value;
use tokio::sync::{mpsc, Mutex, oneshot};

use crate::orchestrator::RunOrchestrator;

/// 默认等待用户响应的超时时间（30 分钟）。
const DEFAULT_ELICITATION_TTL_SECS: u64 = 30 * 60;

/// 内存中的 elicitation 完成通道注册表（`elicitation_id` → oneshot）。
#[derive(Clone)]
pub struct ElicitationBroker {
    inner: Arc<Mutex<HashMap<String, oneshot::Sender<CreateElicitationResult>>>>,
}

impl ElicitationBroker {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// 为新的 elicitation 注册等待通道，返回接收端。
    pub async fn register(&self, id: String) -> oneshot::Receiver<CreateElicitationResult> {
        let (tx, rx) = oneshot::channel();
        self.inner.lock().await.insert(id, tx);
        rx
    }

    /// 用户提交响应后唤醒等待中的 handler；成功返回 `true`。
    pub async fn fulfill(&self, id: &str, result: CreateElicitationResult) -> bool {
        let tx = self.inner.lock().await.remove(id);
        if let Some(tx) = tx {
            tx.send(result).is_ok()
        } else {
            false
        }
    }

    /// 取消等待（如 Run 被中断时清理注册）。
    pub async fn cancel(&self, id: &str) {
        self.inner.lock().await.remove(id);
    }
}

impl Default for ElicitationBroker {
    fn default() -> Self {
        Self::new()
    }
}

/// adk `ElicitationHandler` 实现：落库、切 Run 为 `awaiting_user`、经 SSE 通知前端。
pub struct MacoElicitationHandler {
    /// 当前会话 ID。
    pub session_id: String,
    /// 当前 Run ID。
    pub run_id: String,
    /// 发起 Elicitation 的 MCP 服务名。
    pub mcp_server: String,
    /// Run 状态编排器。
    pub orchestrator: RunOrchestrator,
    /// Elicitation 持久化仓库。
    pub repo: ElicitationRepo,
    /// 等待用户响应的内存 broker。
    pub broker: ElicitationBroker,
    /// SSE 事件发送通道（推送给前端）。
    pub sse_tx: Option<mpsc::Sender<SseEnvelope>>,
}

impl MacoElicitationHandler {
    /// 通用等待流程：写 DB → 暂停 Run → 推 SSE → 阻塞至用户响应或超时。
    async fn wait_for_user(
        &self,
        request_type: &str,
        payload: Value,
        external_id: Option<&str>,
    ) -> Result<CreateElicitationResult, Box<dyn std::error::Error + Send + Sync>> {
        let expires_at = chrono::Utc::now()
            + chrono::Duration::seconds(DEFAULT_ELICITATION_TTL_SECS as i64);
        let expires_at_str = expires_at.to_rfc3339();
        let payload_str = serde_json::to_string(&payload)?;

        let record = self
            .repo
            .insert(
                &self.session_id,
                &self.run_id,
                &self.mcp_server,
                request_type,
                &payload_str,
                &expires_at_str,
                external_id,
            )
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

        let elicitation_id = record.id.clone();

        let resume = ResumeContext {
            schema_version: 1,
            reason: "elicitation".into(),
            parent_run_id: self.run_id.clone(),
            pending_tool_call: None,
            pending_elicitation_id: Some(elicitation_id.clone()),
            user_message_ids: vec![],
            do_not_replay_events: true,
        };
        let resume_raw = serde_json::to_string(&resume)?;
        self.orchestrator
            .await_user(&self.run_id, &resume_raw)
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

        if let Some(tx) = &self.sse_tx {
            let seq = self.orchestrator.next_seq(&self.run_id).await.unwrap_or(0);
            let _ = tx
                .send(SseEnvelope {
                    event_type: "elicitation_request".into(),
                    run_id: self.run_id.clone(),
                    seq,
                    payload: serde_json::json!({
                        "elicitation_id": elicitation_id,
                        "request_type": request_type,
                        "message": payload.get("message"),
                        "schema": payload.get("schema"),
                        "url": payload.get("url"),
                        "mcp_server": self.mcp_server,
                        "status": RUN_STATUS_AWAITING_USER,
                    }),
                })
                .await;
        }

        let rx = self.broker.register(elicitation_id.clone()).await;
        let ttl = Duration::from_secs(DEFAULT_ELICITATION_TTL_SECS);
        let result = match tokio::time::timeout(ttl, rx).await {
            Ok(Ok(res)) => res,
            Ok(Err(_)) => CreateElicitationResult::new(ElicitationAction::Cancel),
            Err(_) => {
                let _ = self.repo.mark_expired(&elicitation_id).await;
                CreateElicitationResult::new(ElicitationAction::Decline)
            }
        };

        let _ = self
            .orchestrator
            .continue_from_awaiting(&self.run_id)
            .await;

        Ok(result)
    }
}

#[async_trait::async_trait]
impl ElicitationHandler for MacoElicitationHandler {
    async fn handle_form_elicitation(
        &self,
        message: &str,
        schema: &ElicitationSchema,
        metadata: Option<&Value>,
    ) -> Result<CreateElicitationResult, Box<dyn std::error::Error + Send + Sync>> {
        let schema_value = serde_json::to_value(schema)?;
        let payload = serde_json::json!({
            "message": message,
            "schema": schema_value,
            "metadata": metadata,
        });
        self.wait_for_user("form", payload, None).await
    }

    async fn handle_url_elicitation(
        &self,
        message: &str,
        url: &str,
        elicitation_id: &str,
        metadata: Option<&Value>,
    ) -> Result<CreateElicitationResult, Box<dyn std::error::Error + Send + Sync>> {
        let payload = serde_json::json!({
            "message": message,
            "url": url,
            "metadata": metadata,
        });
        self.wait_for_user("url", payload, Some(elicitation_id))
            .await
    }
}

/// HTTP `POST /elicitation/:id/respond` 请求体。
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ElicitationRespondBody {
    /// 用户操作：`accept` / `decline` / `cancel`。
    pub action: String,
    /// 用户填写内容（form elicitation 的 accept 时附带）。
    #[serde(default)]
    pub content: Option<Value>,
}

/// 将前端 action 字符串映射为 MCP `ElicitationAction`。
pub fn action_from_str(s: &str) -> Option<ElicitationAction> {
    match s.to_ascii_lowercase().as_str() {
        "accept" => Some(ElicitationAction::Accept),
        "decline" => Some(ElicitationAction::Decline),
        "cancel" => Some(ElicitationAction::Cancel),
        _ => None,
    }
}

/// 组装 MCP 侧 elicitation 结果（可选附带用户填写内容）。
pub fn build_elicitation_result(
    action: ElicitationAction,
    content: Option<Value>,
) -> CreateElicitationResult {
    let mut result = CreateElicitationResult::new(action);
    if let Some(c) = content {
        result = result.with_content(c);
    }
    result
}

/// API 层响应 elicitation：校验状态、写 DB、通过 broker 唤醒等待中的 Run。
pub async fn respond_to_elicitation(
    repo: &ElicitationRepo,
    broker: &ElicitationBroker,
    elicitation_id: &str,
    action: ElicitationAction,
    content: Option<Value>,
) -> MacoResult<bool> {
    let record = repo
        .get(elicitation_id)
        .await?
        .ok_or_else(|| MacoError::not_found("elicitation"))?;
    if record.status != "pending" {
        return Err(MacoError::conflict("elicitation not pending"));
    }

    let status = match action {
        ElicitationAction::Accept => "submitted",
        ElicitationAction::Decline => "cancelled",
        ElicitationAction::Cancel => "cancelled",
    };
    let result = build_elicitation_result(action, content.clone());
    let response_json = serde_json::to_string(&result)
        .map_err(|e| MacoError::config(e.to_string()))?;
    repo.submit_response(elicitation_id, &response_json, status)
        .await?;

    let fulfilled = broker.fulfill(elicitation_id, result).await;
    Ok(fulfilled)
}
