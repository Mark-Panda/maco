//! 将会话事件、Plan、Todo 导出为 Markdown。

use adk_core::{Content, Part};
use adk_session::GetRequest;
use maco_core::{MacoError, MacoResult, APP_NAME, USER_ID};
use maco_db::{PlanRecord, SessionMetaRecord, TodoRecord};
use maco_storage::AdkStorage;

/// 组装完整会话 Markdown 文档（元数据 + plan/todos + 事件时间线）。
pub async fn session_markdown(
    adk: &AdkStorage,
    meta: Option<&SessionMetaRecord>,
    plan: Option<&PlanRecord>,
    todos: &[TodoRecord],
    session_id: &str,
) -> MacoResult<String> {
    let session = adk
        .session
        .get(GetRequest {
            app_name: APP_NAME.into(),
            user_id: USER_ID.into(),
            session_id: session_id.to_string(),
            num_recent_events: None,
            after: None,
        })
        .await
        .map_err(|e| MacoError::Adk(e.to_string()))?;

    let title = meta
        .and_then(|m| m.title.clone())
        .unwrap_or_else(|| "Untitled session".into());
    let mut out = String::new();
    out.push_str(&format!("# {title}\n\n"));
    out.push_str(&format!("- **Session ID**: `{session_id}`\n"));
    if let Some(m) = meta {
        out.push_str(&format!("- **Created**: {}\n", m.created_at));
        out.push_str(&format!("- **Updated**: {}\n", m.updated_at));
        if let Some(ref mid) = m.model_id {
            out.push_str(&format!("- **Model**: `{mid}`\n"));
        }
    }
    out.push('\n');

    if let Some(p) = plan {
        if !p.content.is_empty() {
            out.push_str("## Plan\n\n");
            out.push_str(&p.content);
            out.push_str("\n\n");
        }
    }

    if !todos.is_empty() {
        out.push_str("## Todos\n\n");
        for t in todos {
            let mark = if t.status == "completed" { "x" } else { " " };
            out.push_str(&format!("- [{mark}] **{}** ({}) — {}\n", t.task_key, t.status, t.title));
        }
        out.push('\n');
    }

    out.push_str("## Conversation\n\n");
    for event in session.events().all() {
        if event.author == "maco" {
            continue;
        }
        let Some(content) = event.llm_response.content.as_ref() else {
            continue;
        };
        let text = content_text(content);
        if text.is_empty() {
            continue;
        }
        let role = if event.author == "user" {
            "User"
        } else {
            "Assistant"
        };
        out.push_str(&format!("### {role} ({author})\n\n", author = event.author));
        out.push_str(&text);
        out.push_str("\n\n");
    }

    Ok(out)
}

fn content_text(content: &Content) -> String {
    content
        .parts
        .iter()
        .filter_map(|p| match p {
            Part::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
}
