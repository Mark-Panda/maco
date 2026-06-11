use maco_db::ToolPolicyRecord;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyAction {
    Allow,
    Deny,
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
        assert_eq!(resolve_action(&policies, "tool", "delete_file"), PolicyAction::Allow);
    }

    #[test]
    fn deny_wins() {
        let policies = vec![
            policy("tool", "bash", "confirm"),
            policy("tool", "bash", "deny"),
        ];
        assert_eq!(resolve_action(&policies, "tool", "bash"), PolicyAction::Deny);
    }
}
