//! adk 六类 Callback 工厂：写 `maco_callback_logs`、统计用量，并在 `before_tool` 接入 HITL。

use std::sync::Arc;

use adk_agent::guardrails::{GuardrailSet, PiiRedactor};
use adk_core::{
    AfterAgentCallback, AfterModelCallback, AfterToolCallback, BeforeAgentCallback,
    BeforeModelCallback, BeforeModelResult, BeforeToolCallback,
};
use maco_governance::{pii_guardrail_enabled, prepare_log_payload};
use maco_telemetry::MacoCallbackLogger;

/// 主/子 Agent 共用的 guardrail 集合。
pub fn agent_guardrails() -> GuardrailSet {
    let mut set = GuardrailSet::new();
    if pii_guardrail_enabled() {
        set = set.with(PiiRedactor::new());
    }
    set
}

/// `before_agent`：记录阶段开始，不写 payload。
pub fn before_agent(logger: Arc<MacoCallbackLogger>) -> BeforeAgentCallback {
    Box::new(move |ctx| {
        let logger = Arc::clone(&logger);
        Box::pin(async move {
            logger
                .log_phase("before_agent", ctx.session_id(), None, None)
                .await;
            Ok(None)
        })
    })
}

/// `after_agent`：记录阶段结束。
pub fn after_agent(logger: Arc<MacoCallbackLogger>) -> AfterAgentCallback {
    Box::new(move |ctx| {
        let logger = Arc::clone(&logger);
        Box::pin(async move {
            logger
                .log_phase("after_agent", ctx.session_id(), None, None)
                .await;
            Ok(None)
        })
    })
}

/// `before_model`：脱敏后记录模型请求内容。
pub fn before_model(logger: Arc<MacoCallbackLogger>) -> BeforeModelCallback {
    Box::new(move |ctx, request| {
        let logger = Arc::clone(&logger);
        Box::pin(async move {
            let input = prepare_log_payload(
                &serde_json::to_value(&request.contents).unwrap_or_default(),
            );
            logger
                .log_phase("before_model", ctx.session_id(), Some(&input), None)
                .await;
            Ok(BeforeModelResult::Continue(request))
        })
    })
}

/// `after_model`：记录响应并在终态时写入 `maco_usage_stats`。
pub fn after_model(
    logger: Arc<MacoCallbackLogger>,
    usage: Option<crate::usage::SharedUsageContext>,
) -> AfterModelCallback {
    Box::new(move |ctx, response| {
        let logger = Arc::clone(&logger);
        let usage = usage.clone();
        let resp = response.clone();
        Box::pin(async move {
            let output = prepare_log_payload(
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

use maco_core::{worktree_mcp_path_access_denied, SessionWorkspace};

use crate::hitl::{tool_denied_content, HitlGate};

/// `before_tool`：写工具调用日志，并按策略触发 HITL 确认。
pub fn before_tool_with_hitl(
    logger: Arc<MacoCallbackLogger>,
    hitl: Arc<HitlGate>,
    workspace: Option<SessionWorkspace>,
    worktree_path_guard: bool,
) -> BeforeToolCallback {
    Box::new(move |ctx| {
        let logger = Arc::clone(&logger);
        let hitl = Arc::clone(&hitl);
        let workspace = workspace.clone();
        Box::pin(async move {
            let tool_name = ctx.tool_name().unwrap_or("unknown");
            let input = ctx
                .tool_input()
                .map(|v| prepare_log_payload(v))
                .unwrap_or_else(|| "{}".into());
            logger.log_tool_start(tool_name, &input).await;

            let args = ctx.tool_input().cloned().unwrap_or(serde_json::json!({}));
            let call_id = ctx.invocation_id().to_string();

            if worktree_path_guard {
                if let Some(ws) = workspace.as_ref() {
                    if let Some(reason) = worktree_mcp_path_access_denied(
                        ws.uses_worktree,
                        &ws.repo_root,
                        &ws.workspace_root,
                        tool_name,
                        &args,
                    ) {
                        return Ok(Some(tool_denied_content(tool_name, &call_id, reason)));
                    }
                }
            }

            let source = if tool_name.contains("__") {
                "mcp"
            } else if ctx.tool_name().map(|n| n.starts_with("update_") || n == "upsert_todo").unwrap_or(false) {
                "tool"
            } else {
                "tool"
            };
            if let Some(content) = hitl
                .check_before_tool(source, tool_name, &args, &call_id)
                .await?
            {
                return Ok(Some(content));
            }
            Ok(None)
        })
    })
}

/// `after_tool`：记录工具执行结果与耗时。
pub fn after_tool(logger: Arc<MacoCallbackLogger>) -> AfterToolCallback {
    Box::new(move |ctx| {
        let logger = Arc::clone(&logger);
        Box::pin(async move {
            let tool_name = ctx.tool_name().unwrap_or("unknown");
            let outcome = ctx.tool_outcome();
            let error_message = outcome.as_ref().and_then(|o| o.error_message.clone());
            let output = outcome
                .as_ref()
                .map(|o| {
                    prepare_log_payload(&serde_json::json!({
                        "success": o.success,
                        "duration_ms": o.duration.as_millis(),
                    }))
                })
                .unwrap_or_else(|| "{}".into());
            logger
                .log_tool_end(
                    tool_name,
                    &output,
                    error_message.as_deref(),
                )
                .await;
            Ok(None)
        })
    })
}
