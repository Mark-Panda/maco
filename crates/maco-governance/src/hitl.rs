//! 工具调用 HITL：根据 `ToolPolicyRecord` 与会话权限模式解析 allow / confirm / deny。

use maco_core::AgentPermissionMode;
use maco_db::ToolPolicyRecord;

/// 工具策略判定结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyAction {
    /// 直接执行。
    Allow,
    /// 拒绝执行。
    Deny,
    /// 需用户确认后执行（HITL）。
    Confirm,
}

impl PolicyAction {
    pub fn parse(s: &str) -> Self {
        match s {
            "deny" => Self::Deny,
            "confirm" => Self::Confirm,
            _ => Self::Allow,
        }
    }
}

/// Match `source_type` + `tool_name` against policy rows.
/// Patterns: `delete_*` (tool_pattern only) or full `mcp:server:tool` style via source_type column.
pub fn resolve_action(
    policies: &[ToolPolicyRecord],
    source_type: &str,
    tool_name: &str,
) -> PolicyAction {
    let mut best = PolicyAction::Allow;
    for p in policies {
        if p.source_type != "*" && p.source_type != source_type {
            continue;
        }
        if wildcard_match(&p.tool_pattern, tool_name) {
            let action = PolicyAction::parse(&p.action);
            if action == PolicyAction::Deny {
                return PolicyAction::Deny;
            }
            if action == PolicyAction::Confirm {
                best = PolicyAction::Confirm;
            }
        }
    }
    best
}

/// 结合会话权限模式与工具策略，得到最终 HITL 动作。
pub fn resolve_action_with_mode(
    policies: &[ToolPolicyRecord],
    mode: AgentPermissionMode,
    source_type: &str,
    tool_name: &str,
) -> PolicyAction {
    let base = resolve_action(policies, source_type, tool_name);
    if base == PolicyAction::Deny {
        return PolicyAction::Deny;
    }
    match mode {
        AgentPermissionMode::FullAccess => PolicyAction::Allow,
        AgentPermissionMode::AutoApprove => base,
        AgentPermissionMode::RequestApproval => {
            if needs_request_approval(source_type, tool_name) {
                PolicyAction::Confirm
            } else {
                base
            }
        }
    }
}

/// `request_approval` 模式下需额外确认的工具：shell、MCP（含外网与外部文件）。
fn needs_request_approval(source_type: &str, tool_name: &str) -> bool {
    if tool_name == "bash" {
        return true;
    }
    if source_type == "mcp" || tool_name.contains("__") {
        return true;
    }
    false
}

fn wildcard_match(pattern: &str, name: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if let Some(prefix) = pattern.strip_suffix('*') {
        return name.starts_with(prefix);
    }
    pattern == name
}

#[cfg(test)]
mod tests {
    use super::*;
    use maco_core::AgentPermissionMode;

    fn policy(source: &str, pattern: &str, action: &str) -> ToolPolicyRecord {
        ToolPolicyRecord {
            id: "1".into(),
            tool_pattern: pattern.into(),
            source_type: source.into(),
            action: action.into(),
            enabled: 1,
            created_at: "".into(),
        }
    }

    #[test]
    fn confirm_delete_tools() {
        let policies = vec![policy("mcp", "delete_*", "confirm")];
        assert_eq!(
            resolve_action(&policies, "mcp", "delete_file"),
            PolicyAction::Confirm
        );
        assert_eq!(
            resolve_action(&policies, "tool", "delete_file"),
            PolicyAction::Allow
        );
    }

    #[test]
    fn deny_wins() {
        let policies = vec![
            policy("tool", "bash", "confirm"),
            policy("tool", "bash", "deny"),
        ];
        assert_eq!(
            resolve_action(&policies, "tool", "bash"),
            PolicyAction::Deny
        );
    }

    #[test]
    fn full_access_skips_confirm() {
        let policies = vec![policy("tool", "bash", "confirm")];
        assert_eq!(
            resolve_action_with_mode(&policies, AgentPermissionMode::FullAccess, "tool", "bash",),
            PolicyAction::Allow
        );
    }

    #[test]
    fn request_approval_confirms_bash_and_mcp() {
        let policies = vec![];
        assert_eq!(
            resolve_action_with_mode(
                &policies,
                AgentPermissionMode::RequestApproval,
                "tool",
                "bash",
            ),
            PolicyAction::Confirm
        );
        assert_eq!(
            resolve_action_with_mode(
                &policies,
                AgentPermissionMode::RequestApproval,
                "mcp",
                "read_file",
            ),
            PolicyAction::Confirm
        );
        assert_eq!(
            resolve_action_with_mode(
                &policies,
                AgentPermissionMode::RequestApproval,
                "tool",
                "update_plan",
            ),
            PolicyAction::Allow
        );
    }

    #[test]
    fn auto_approve_uses_policy_only() {
        let policies = vec![policy("mcp", "delete_*", "confirm")];
        assert_eq!(
            resolve_action_with_mode(
                &policies,
                AgentPermissionMode::AutoApprove,
                "mcp",
                "read_file",
            ),
            PolicyAction::Allow
        );
        assert_eq!(
            resolve_action_with_mode(
                &policies,
                AgentPermissionMode::AutoApprove,
                "mcp",
                "delete_file",
            ),
            PolicyAction::Confirm
        );
    }
}
