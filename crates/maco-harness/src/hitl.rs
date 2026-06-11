//! Human-in-the-loop：危险工具执行前暂停 Run，等待用户确认。

use adk_core::{Content, FunctionResponseData, Part};
use adk_core::Result as AdkResult;
use maco_core::{ResumeContext, SseEnvelope, RUN_STATUS_AWAITING_USER};
use maco_db::ToolPolicyRecord;
use maco_governance::{resolve_action, PolicyAction};
use tokio::sync::mpsc;

use crate::orchestrator::RunOrchestrator;
use crate::run_stream::RunStreamRegistry;

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
}

impl HitlGate {
    /// 根据 `maco_tool_policies` 判定 allow/deny/confirm；confirm 时返回占位响应并暂停 Run。
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
            PolicyAction::Deny => Ok(Some(denied_content(tool_name, call_id, "tool denied by policy"))),
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

                Ok(Some(pending_content(tool_name, call_id)))
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

/// 构造等待用户确认时的占位 FunctionResponse。
fn pending_content(tool_name: &str, call_id: &str) -> Content {
    Content {
        role: "user".into(),
        parts: vec![Part::FunctionResponse {
            function_response: FunctionResponseData::new(
                tool_name,
                serde_json::json!({
                    "status": "awaiting_user_confirmation",
                    "message": "User confirmation required before executing this tool"
                }),
            ),
            id: Some(call_id.to_string()),
        }],
    }
}

/// 将用户对工具确认的裁决编码为可续跑的 `Content`。
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
