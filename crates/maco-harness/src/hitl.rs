use adk_core::{Content, FunctionResponseData, Part};
use adk_core::Result as AdkResult;
use maco_core::{ResumeContext, SseEnvelope, RUN_STATUS_AWAITING_USER};
use maco_db::ToolPolicyRecord;
use maco_governance::{resolve_action, PolicyAction};
use tokio::sync::mpsc;

use crate::orchestrator::RunOrchestrator;

pub struct HitlGate {
    pub run_id: String,
    pub session_id: String,
    pub orchestrator: RunOrchestrator,
    pub policies: Vec<ToolPolicyRecord>,
    pub sse_tx: Option<mpsc::Sender<SseEnvelope>>,
}

impl HitlGate {
    pub async fn check_before_tool(
        &self,
        source_type: &str,
        tool_name: &str,
        tool_args: &serde_json::Value,
        call_id: &str,
    ) -> AdkResult<Option<Content>> {
        match resolve_action(&self.policies, source_type, tool_name) {
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

                if let Some(tx) = &self.sse_tx {
                    let seq = self.orchestrator.next_seq(&self.run_id).await.unwrap_or(0);
                    let _ = tx
                        .send(SseEnvelope {
                            event_type: "tool_confirm_request".into(),
                            run_id: self.run_id.clone(),
                            seq,
                            payload: serde_json::json!({
                                "tool_name": tool_name,
                                "args": tool_args,
                                "call_id": call_id,
                                "status": RUN_STATUS_AWAITING_USER,
                            }),
                        })
                        .await;
                }

                Ok(Some(pending_content(tool_name, call_id)))
            }
        }
    }
}

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
