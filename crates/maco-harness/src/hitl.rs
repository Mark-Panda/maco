//! Human-in-the-loop：危险工具执行前暂停 Run，等待用户确认。

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use adk_core::{Content, FunctionResponseData, Part};
use adk_core::Result as AdkResult;
use maco_core::{ResumeContext, SseEnvelope, RUN_STATUS_AWAITING_USER};
use maco_db::ToolPolicyRecord;
use maco_governance::{resolve_action, PolicyAction};
use tokio::sync::{mpsc, Mutex, oneshot};

use crate::orchestrator::RunOrchestrator;
use crate::run_stream::RunStreamRegistry;

/// 默认 HITL 等待用户确认的超时时间（30 分钟）。
const DEFAULT_HITL_TTL_SECS: u64 = 30 * 60;

/// 内存中的 HITL 完成通道（`run_id` → 是否批准）。
#[derive(Clone, Default)]
pub struct HitlBroker {
    inner: Arc<Mutex<HashMap<String, oneshot::Sender<bool>>>>,
}

impl HitlBroker {
    pub fn new() -> Self {
        Self::default()
    }

    /// 为 Run 注册等待通道，返回接收端。
    pub async fn register(&self, run_id: &str) -> oneshot::Receiver<bool> {
        let (tx, rx) = oneshot::channel();
        self.inner
            .lock()
            .await
            .insert(run_id.to_string(), tx);
        rx
    }

    /// 用户提交确认后唤醒等待中的 `before_tool`；成功返回 `true`。
    pub async fn fulfill(&self, run_id: &str, approved: bool) -> bool {
        let tx = self.inner.lock().await.remove(run_id);
        if let Some(tx) = tx {
            tx.send(approved).is_ok()
        } else {
            false
        }
    }

    /// 取消等待（超时或 Run 被中断时清理）。
    pub async fn cancel(&self, run_id: &str) {
        self.inner.lock().await.remove(run_id);
    }
}

/// 工具执行前的策略闸门，命中 `confirm` 时挂起 Run 并推送 SSE 确认事件。
pub struct HitlGate {
    /// 当前 Run ID。
    pub run_id: String,
    /// 当前会话 ID。
    pub session_id: String,
    /// Run 状态编排器。
    pub orchestrator: RunOrchestrator,
    /// 启用的工具策略规则。
    pub policies: Vec<ToolPolicyRecord>,
    /// SSE 推送通道（通知前端待审批工具）。
    pub sse_tx: Option<mpsc::Sender<SseEnvelope>>,
    /// 广播注册表（SSE 重连）。
    pub stream: Option<RunStreamRegistry>,
    /// 跨 HTTP 请求唤醒同 Run 内阻塞的 `before_tool`。
    pub broker: HitlBroker,
}

impl HitlGate {
    /// 根据 `maco_tool_policies` 判定 allow/deny/confirm；confirm 时阻塞至用户批准/拒绝。
    pub async fn check_before_tool(
        &self,
        source_type: &str,
        tool_name: &str,
        tool_args: &serde_json::Value,
        call_id: &str,
    ) -> AdkResult<Option<Content>> {
        let (policy_source, policy_tool) = if let Some((_, short)) = tool_name.split_once("__") {
            ("mcp", short)
        } else {
            (source_type, tool_name)
        };
        match resolve_action(&self.policies, policy_source, policy_tool) {
            PolicyAction::Allow => Ok(None),
            PolicyAction::Deny => Ok(Some(denied_content(
                tool_name,
                call_id,
                "tool denied by policy",
            ))),
            PolicyAction::Confirm => {
                let resume = ResumeContext {
                    schema_version: 1,
                    reason: "hitl".into(),
                    parent_run_id: self.run_id.clone(),
                    pending_tool_call: Some(maco_core::PendingToolCall {
                        name: tool_name.to_string(),
                        args: tool_args.clone(),
                        call_id: call_id.to_string(),
                    }),
                    pending_elicitation_id: None,
                    user_message_ids: vec![],
                    do_not_replay_events: true,
                };
                let raw = serde_json::to_string(&resume)
                    .map_err(|e| adk_core::AdkError::config(e.to_string()))?;
                self.orchestrator
                    .await_user(&self.run_id, &raw)
                    .await
                    .map_err(|e| adk_core::AdkError::config(e.to_string()))?;

                let seq = self.orchestrator.next_seq(&self.run_id).await.unwrap_or(0);
                let env = SseEnvelope {
                    event_type: "tool_confirm_request".into(),
                    run_id: self.run_id.clone(),
                    seq,
                    payload: serde_json::json!({
                        "tool_name": tool_name,
                        "args": tool_args,
                        "call_id": call_id,
                        "status": RUN_STATUS_AWAITING_USER,
                    }),
                };
                if let Some(tx) = &self.sse_tx {
                    let _ = tx.send(env.clone()).await;
                }
                if let Some(reg) = &self.stream {
                    reg.publish(&self.session_id, env).await;
                }

                let rx = self.broker.register(&self.run_id).await;
                let ttl = Duration::from_secs(DEFAULT_HITL_TTL_SECS);
                let approved = match tokio::time::timeout(ttl, rx).await {
                    Ok(Ok(value)) => value,
                    Ok(Err(_)) | Err(_) => false,
                };
                self.broker.cancel(&self.run_id).await;

                self.orchestrator
                    .continue_from_awaiting(&self.run_id)
                    .await
                    .map_err(|e| adk_core::AdkError::config(e.to_string()))?;

                if approved {
                    Ok(None)
                } else {
                    Ok(Some(denied_content(
                        tool_name,
                        call_id,
                        "User rejected tool execution",
                    )))
                }
            }
        }
    }
}

/// 构造工具被拒绝时的 FunctionResponse 内容。
fn denied_content(tool_name: &str, call_id: &str, message: &str) -> Content {
    Content {
        role: "user".into(),
        parts: vec![Part::FunctionResponse {
            function_response: FunctionResponseData::new(
                tool_name,
                serde_json::json!({ "error": message, "denied": true }),
            ),
            id: Some(call_id.to_string()),
        }],
    }
}

/// 将用户对工具确认的裁决编码为可续跑的 `Content`（仅用于断线 fallback）。
pub fn build_resume_content(
    tool_name: &str,
    call_id: &str,
    approved: bool,
    note: Option<&str>,
) -> Content {
    let payload = if approved {
        serde_json::json!({
            "status": "approved",
            "message": note.unwrap_or("User approved tool execution")
        })
    } else {
        serde_json::json!({
            "status": "rejected",
            "denied": true,
            "message": note.unwrap_or("User rejected tool execution")
        })
    };
    Content {
        role: "user".into(),
        parts: vec![Part::FunctionResponse {
            function_response: FunctionResponseData::new(tool_name, payload),
            id: Some(call_id.to_string()),
        }],
    }
}

/// 注入真实工具执行结果（断线 fallback，已批准时）。
pub fn build_tool_result_content(
    tool_name: &str,
    call_id: &str,
    result: serde_json::Value,
) -> Content {
    Content {
        role: "user".into(),
        parts: vec![Part::FunctionResponse {
            function_response: FunctionResponseData::new(tool_name, result),
            id: Some(call_id.to_string()),
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn hitl_broker_fulfill_wakes_waiter() {
        let broker = HitlBroker::new();
        let rx = broker.register("run-1").await;
        assert!(broker.fulfill("run-1", true).await);
        assert!(rx.await.unwrap());
    }

    #[tokio::test]
    async fn hitl_broker_fulfill_returns_false_when_missing() {
        let broker = HitlBroker::new();
        assert!(!broker.fulfill("missing", true).await);
    }
}
