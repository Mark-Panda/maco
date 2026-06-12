//! ADK `ContextCoordinator` + `ToolRegistry`：Skill 选择与 `allowed-tools` 工具绑定。

use std::collections::HashMap;
use std::sync::Arc;

use adk_core::{Content, Part, Tool, ToolRegistry, Toolset};
use adk_skill::{
    ContextCoordinator, CoordinatorConfig, SkillContext, SkillIndex, ValidationMode,
};
use adk_tool::{LoadArtifactsTool, SimpleToolContext};
use maco_core::{MacoError, MacoResult};
use maco_react::ReactTools;
use maco_storage::adk_artifacts_enabled;
use tracing::info;

use crate::adk_skills::default_selection_policy;
/// maco 侧工具注册表：静态工具 + MCP toolset 展开后的名称映射。
pub struct MacoToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl MacoToolRegistry {
    /// 收集 ReAct、bash 与全部 MCP toolset 中的工具实例。
    pub async fn build(
        react: &ReactTools,
        bash: Arc<dyn Tool>,
        toolsets: &[Arc<dyn Toolset>],
    ) -> MacoResult<Self> {
        let mut tools: HashMap<String, Arc<dyn Tool>> = HashMap::new();

        for tool in react.as_tool_arcs() {
            tools.insert(tool.name().to_string(), tool);
        }
        tools.insert(bash.name().to_string(), bash);
        if adk_artifacts_enabled() {
            let load: Arc<dyn Tool> = Arc::new(LoadArtifactsTool::new());
            tools.insert(load.name().to_string(), load);
        }

        let scan_ctx = Arc::new(SimpleToolContext::new("maco-tool-registry-scan"));
        for toolset in toolsets {
            let listed = toolset
                .tools(scan_ctx.clone())
                .await
                .map_err(|e| MacoError::Adk(e.to_string()))?;
            for tool in listed {
                tools.insert(tool.name().to_string(), tool);
            }
        }

        Ok(Self { tools })
    }

    pub fn len(&self) -> usize {
        self.tools.len()
    }
}

impl ToolRegistry for MacoToolRegistry {
    fn resolve(&self, tool_name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(tool_name).cloned()
    }

    fn available_tools(&self) -> Vec<String> {
        let mut names: Vec<String> = self.tools.keys().cloned().collect();
        names.sort();
        names
    }
}

/// 默认 Coordinator 配置（与历史 `with_skill_budget(8000)` 对齐）。
pub fn default_coordinator_config() -> CoordinatorConfig {
    CoordinatorConfig {
        policy: default_selection_policy(),
        max_instruction_chars: 8_000,
        validation_mode: ValidationMode::Strict,
    }
}

/// 从用户消息提取用于 Skill 评分的文本。
pub fn extract_user_query(content: &Content) -> String {
    content
        .parts
        .iter()
        .filter_map(|part| match part {
            Part::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// 运行 ContextCoordinator 流水线：评分 → 校验 allowed-tools → 构造 SkillContext。
pub fn resolve_skill_context(
    index: &SkillIndex,
    registry: Arc<MacoToolRegistry>,
    query: &str,
    config: &CoordinatorConfig,
) -> Option<SkillContext> {
    if query.trim().is_empty() {
        return None;
    }
    let coordinator = ContextCoordinator::new(Arc::new(index.clone()), registry, config.clone());
    let ctx = coordinator.build_context(query)?;
    info!(
        skill = %ctx.provenance.skill.name,
        score = ctx.provenance.score,
        allowed = ?ctx.provenance.skill.allowed_tools,
        active_tools = ctx.active_tools.len(),
        "skill context resolved via ContextCoordinator"
    );
    Some(ctx)
}

/// Skill 是否声明了 `allowed-tools` 约束。
pub fn skill_restricts_tools(ctx: &SkillContext) -> bool {
    !ctx.provenance.skill.allowed_tools.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use adk_core::ToolContext;
    use async_trait::async_trait;
    use serde_json::Value;

    struct StubTool {
        name: &'static str,
    }

    #[async_trait]
    impl Tool for StubTool {
        fn name(&self) -> &str {
            self.name
        }

        fn description(&self) -> &str {
            "stub"
        }

        async fn execute(
            &self,
            _ctx: Arc<dyn ToolContext>,
            _args: Value,
        ) -> adk_core::Result<Value> {
            Ok(Value::Null)
        }
    }

    #[test]
    fn registry_resolves_by_name() {
        let mut tools = HashMap::new();
        let bash: Arc<dyn Tool> = Arc::new(StubTool { name: "bash" });
        tools.insert("bash".to_string(), bash.clone());
        let registry = MacoToolRegistry { tools };
        assert!(registry.resolve("bash").is_some());
        assert!(registry.resolve("missing").is_none());
        assert_eq!(registry.available_tools(), vec!["bash".to_string()]);
    }

    #[test]
    fn extract_user_query_joins_text_parts() {
        let content = Content {
            role: "user".into(),
            parts: vec![
                Part::Text {
                    text: "hello".into(),
                },
                Part::Text {
                    text: "world".into(),
                },
            ],
        };
        assert_eq!(extract_user_query(&content), "hello\nworld");
    }
}
