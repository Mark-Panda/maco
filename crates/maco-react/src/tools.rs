//! ReAct Agent 工具：`update_plan` / `upsert_todo` 写入 SQLite。

use std::sync::Arc;

use adk_core::{Result as AdkResult, Tool, ToolContext};
use async_trait::async_trait;
use maco_db::ReactRepo;
use serde_json::{json, Value};

/// 将 `ReactRepo` 暴露为 adk `Tool` 列表。
pub struct ReactTools {
    /// 底层 ReAct 数据仓库。
    pub repo: ReactRepo,
}

impl ReactTools {
    pub fn new(repo: ReactRepo) -> Self {
        Self { repo }
    }

    /// 返回可挂到 `LlmAgentBuilder` 的工具集合。
    pub fn as_tool_arcs(&self) -> Vec<Arc<dyn Tool>> {
        vec![
            Arc::new(UpdatePlanTool {
                repo: self.repo.clone(),
            }),
            Arc::new(UpsertTodoTool {
                repo: self.repo.clone(),
            }),
        ]
    }
}

/// Agent 工具：更新会话任务计划。
struct UpdatePlanTool {
    /// ReAct 仓库。
    repo: ReactRepo,
}

#[async_trait]
impl Tool for UpdatePlanTool {
    fn name(&self) -> &str {
        "update_plan"
    }

    fn description(&self) -> &str {
        "Update the session task plan markdown. Use when breaking work into steps."
    }

    fn parameters_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "content": { "type": "string", "description": "Full plan markdown" }
            },
            "required": ["content"]
        }))
    }

    async fn execute(&self, ctx: Arc<dyn ToolContext>, args: Value) -> AdkResult<Value> {
        let content = args
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let plan = self
            .repo
            .upsert_plan(ctx.session_id(), content, None)
            .await
            .map_err(|e| {
                adk_core::AdkError::new(
                    adk_core::ErrorComponent::Tool,
                    adk_core::ErrorCategory::Internal,
                    "maco.react.update_plan",
                    e.to_string(),
                )
            })?;
        Ok(json!({
            "session_id": plan.session_id,
            "version": plan.version,
            "updated_at": plan.updated_at
        }))
    }
}

/// Agent 工具：创建或更新待办项。
struct UpsertTodoTool {
    /// ReAct 仓库。
    repo: ReactRepo,
}

#[async_trait]
impl Tool for UpsertTodoTool {
    fn name(&self) -> &str {
        "upsert_todo"
    }

    fn description(&self) -> &str {
        "Create or update a todo item for the session."
    }

    fn parameters_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "task_key": { "type": "string", "description": "Stable todo identifier" },
                "title": { "type": "string", "description": "Human-readable title" },
                "sort_order": { "type": "integer", "description": "Display order", "default": 0 }
            },
            "required": ["task_key", "title"]
        }))
    }

    async fn execute(&self, ctx: Arc<dyn ToolContext>, args: Value) -> AdkResult<Value> {
        let task_key = args
            .get("task_key")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let title = args.get("title").and_then(|v| v.as_str()).unwrap_or("");
        let sort_order = args
            .get("sort_order")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let todo = self
            .repo
            .upsert_todo(ctx.session_id(), task_key, title, sort_order)
            .await
            .map_err(|e| {
                adk_core::AdkError::new(
                    adk_core::ErrorComponent::Tool,
                    adk_core::ErrorCategory::Internal,
                    "maco.react.update_plan",
                    e.to_string(),
                )
            })?;
        Ok(json!({
            "task_key": todo.task_key,
            "title": todo.title,
            "status": todo.status,
            "sort_order": todo.sort_order
        }))
    }
}
