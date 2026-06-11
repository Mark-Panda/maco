use std::sync::Arc;

use adk_core::{
    AfterAgentCallback, AfterModelCallback, AfterToolCallback, BeforeAgentCallback,
    BeforeModelCallback, BeforeModelResult, BeforeToolCallback,
};
use maco_governance::prepare_log_payload;
use maco_telemetry::MacoCallbackLogger;

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

use crate::hitl::HitlGate;

pub fn before_tool_with_hitl(
    logger: Arc<MacoCallbackLogger>,
    hitl: Arc<HitlGate>,
) -> BeforeToolCallback {
    Box::new(move |ctx| {
        let logger = Arc::clone(&logger);
        let hitl = Arc::clone(&hitl);
        Box::pin(async move {
            let tool_name = ctx.tool_name().unwrap_or("unknown");
            let input = ctx
                .tool_input()
                .map(|v| prepare_log_payload(v))
                .unwrap_or_else(|| "{}".into());
            logger.log_tool_start(tool_name, &input).await;

            let args = ctx.tool_input().cloned().unwrap_or(serde_json::json!({}));
            let call_id = ctx.invocation_id().to_string();
            if let Some(content) = hitl
                .check_before_tool("tool", tool_name, &args, &call_id)
                .await?
            {
                return Ok(Some(content));
            }
            Ok(None)
        })
    })
}

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
