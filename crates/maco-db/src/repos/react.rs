//! ReAct 计划与待办持久化（`maco_react_plans` / `maco_react_todos`）。

use maco_core::{MacoError, MacoResult};
use sqlx::SqlitePool;
use uuid::Uuid;

/// 会话级任务计划（Markdown 全文 + 乐观锁版本号）。
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct PlanRecord {
    /// 所属会话 ID。
    pub session_id: String,
    /// 计划 Markdown 全文。
    pub content: String,
    /// 乐观锁版本号。
    pub version: i64,
    /// 最后更新时间。
    pub updated_at: String,
}

/// 会话下单条待办（由 `task_key` 唯一标识）。
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct TodoRecord {
    /// 记录 ID。
    pub id: String,
    /// 所属会话 ID。
    pub session_id: String,
    /// 稳定业务键（同会话内唯一）。
    pub task_key: String,
    /// 待办标题。
    pub title: String,
    /// 状态（`pending` / `in_progress` / `done` 等）。
    pub status: String,
    /// 展示排序权重。
    pub sort_order: i64,
    /// 创建时间。
    pub created_at: String,
    /// 最后更新时间。
    pub updated_at: String,
}

/// ReAct 计划/待办的数据访问层。
#[derive(Clone)]
pub struct ReactRepo {
    pool: SqlitePool,
}

impl ReactRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// 读取指定会话的计划；不存在时返回 `None`。
    pub async fn get_plan(&self, session_id: &str) -> MacoResult<Option<PlanRecord>> {
        sqlx::query_as::<_, PlanRecord>(
            "SELECT session_id, content, version, updated_at FROM maco_react_plans WHERE session_id = ?",
        )
        .bind(session_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| MacoError::database(e.to_string()))
    }

    /// 创建或更新计划；可选 `expected_version` 用于乐观锁冲突检测。
    pub async fn upsert_plan(
        &self,
        session_id: &str,
        content: &str,
        expected_version: Option<i64>,
    ) -> MacoResult<PlanRecord> {
        if let Some(existing) = self.get_plan(session_id).await? {
            if let Some(v) = expected_version
                && existing.version != v
            {
                return Err(MacoError::conflict(format!(
                    "plan version mismatch: expected {v}, got {}",
                    existing.version
                )));
            }
            let new_version = existing.version + 1;
            let now = chrono::Utc::now().to_rfc3339();
            sqlx::query(
                "UPDATE maco_react_plans SET content = ?, version = ?, updated_at = ? WHERE session_id = ?",
            )
            .bind(content)
            .bind(new_version)
            .bind(&now)
            .bind(session_id)
            .execute(&self.pool)
            .await
            .map_err(|e| MacoError::database(e.to_string()))?;
            return Ok(PlanRecord {
                session_id: session_id.to_string(),
                content: content.to_string(),
                version: new_version,
                updated_at: now,
            });
        }
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO maco_react_plans (session_id, content, version, updated_at) VALUES (?, ?, 1, ?)",
        )
        .bind(session_id)
        .bind(content)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(|e| MacoError::database(e.to_string()))?;
        Ok(PlanRecord {
            session_id: session_id.to_string(),
            content: content.to_string(),
            version: 1,
            updated_at: now,
        })
    }

    /// 列出会话下全部待办，按 `sort_order` 排序。
    pub async fn list_todos(&self, session_id: &str) -> MacoResult<Vec<TodoRecord>> {
        sqlx::query_as::<_, TodoRecord>(
            "SELECT id, session_id, task_key, title, status, sort_order, created_at, updated_at
             FROM maco_react_todos WHERE session_id = ? ORDER BY sort_order",
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| MacoError::database(e.to_string()))
    }

    /// 更新待办状态（pending / in_progress / completed 等）。
    pub async fn patch_todo_status(
        &self,
        session_id: &str,
        task_key: &str,
        status: &str,
    ) -> MacoResult<TodoRecord> {
        let now = chrono::Utc::now().to_rfc3339();
        let rows = sqlx::query(
            "UPDATE maco_react_todos SET status = ?, updated_at = ? WHERE session_id = ? AND task_key = ?",
        )
        .bind(status)
        .bind(&now)
        .bind(session_id)
        .bind(task_key)
        .execute(&self.pool)
        .await
        .map_err(|e| MacoError::database(e.to_string()))?;
        if rows.rows_affected() == 0 {
            return Err(MacoError::not_found("todo not found"));
        }
        sqlx::query_as::<_, TodoRecord>(
            "SELECT id, session_id, task_key, title, status, sort_order, created_at, updated_at
             FROM maco_react_todos WHERE session_id = ? AND task_key = ?",
        )
        .bind(session_id)
        .bind(task_key)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| MacoError::database(e.to_string()))
    }

    /// Agent 工具 `upsert_todo` 落库：按 `(session_id, task_key)` 插入或更新标题/状态。
    pub async fn upsert_todo(
        &self,
        session_id: &str,
        task_key: &str,
        title: &str,
        sort_order: i64,
        status: Option<&str>,
    ) -> MacoResult<TodoRecord> {
        let now = chrono::Utc::now().to_rfc3339();
        let status = normalize_todo_status(status.unwrap_or("pending"));
        if let Ok(existing) = sqlx::query_as::<_, TodoRecord>(
            "SELECT id, session_id, task_key, title, status, sort_order, created_at, updated_at
             FROM maco_react_todos WHERE session_id = ? AND task_key = ?",
        )
        .bind(session_id)
        .bind(task_key)
        .fetch_one(&self.pool)
        .await
        {
            sqlx::query(
                "UPDATE maco_react_todos SET title = ?, status = ?, sort_order = ?, updated_at = ? WHERE id = ?",
            )
            .bind(title)
            .bind(&status)
            .bind(sort_order)
            .bind(&now)
            .bind(&existing.id)
            .execute(&self.pool)
            .await
            .map_err(|e| MacoError::database(e.to_string()))?;
            return Ok(TodoRecord {
                title: title.to_string(),
                status,
                sort_order,
                updated_at: now,
                ..existing
            });
        }
        let id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO maco_react_todos (id, session_id, task_key, title, status, sort_order, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(session_id)
        .bind(task_key)
        .bind(title)
        .bind(&status)
        .bind(sort_order)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(|e| MacoError::database(e.to_string()))?;
        Ok(TodoRecord {
            id,
            session_id: session_id.to_string(),
            task_key: task_key.to_string(),
            title: title.to_string(),
            status,
            sort_order,
            created_at: now.clone(),
            updated_at: now,
        })
    }

    /// 根据 plan Markdown 中的 checkbox 同步待办状态（`[x]` → completed）。
    pub async fn sync_todo_status_from_plan(
        &self,
        session_id: &str,
        plan_content: &str,
    ) -> MacoResult<()> {
        let todos = self.list_todos(session_id).await?;
        if todos.is_empty() {
            return Ok(());
        }
        let checkboxes = parse_plan_checkbox_items(plan_content);
        if checkboxes.is_empty() {
            return Ok(());
        }
        for todo in todos {
            let Some(status) = match_checkbox_status(&checkboxes, &todo.title) else {
                continue;
            };
            if todo.status == status {
                continue;
            }
            // plan 可能滞后于 upsert_todo；禁止用未勾选 checkbox 覆盖更高进度。
            if todo_status_rank(&status) <= todo_status_rank(&todo.status) {
                continue;
            }
            self.patch_todo_status(session_id, &todo.task_key, &status)
                .await?;
        }
        Ok(())
    }
}

/// 待办状态优先级（completed > in_progress > pending）。
fn todo_status_rank(status: &str) -> u8 {
    match normalize_todo_status(status).as_str() {
        "completed" => 3,
        "in_progress" => 2,
        _ => 1,
    }
}

/// 规范化待办状态字符串。
pub fn normalize_todo_status(raw: &str) -> String {
    match raw.trim().to_lowercase().as_str() {
        "done" | "complete" | "completed" | "finished" => "completed".into(),
        "in_progress" | "in-progress" | "doing" | "active" | "started" => "in_progress".into(),
        _ => "pending".into(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PlanCheckboxItem {
    title: String,
    status: String,
}

fn normalize_title_key(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_whitespace() && !['：', ':', '，', ',', '。', '.', '、'].contains(c))
        .flat_map(|c| c.to_lowercase())
        .collect()
}

fn parse_plan_checkbox_items(content: &str) -> Vec<PlanCheckboxItem> {
    let mut items = Vec::new();
    for line in content.lines() {
        if let Some(item) = parse_checkbox_line(line.trim()) {
            items.push(item);
        }
    }
    items
}

fn parse_checkbox_line(line: &str) -> Option<PlanCheckboxItem> {
    let open = line.find('[')?;
    let rest = &line[open..];
    let close = rest.find(']')?;
    if close <= 1 {
        return None;
    }
    let mark = rest[1..close].trim();
    let title = rest[close + 1..].trim();
    if title.is_empty() {
        return None;
    }
    let status = match mark {
        "x" | "X" => "completed",
        "~" | "/" => "in_progress",
        _ => "pending",
    };
    Some(PlanCheckboxItem {
        title: title.to_string(),
        status: status.into(),
    })
}

fn match_checkbox_status(items: &[PlanCheckboxItem], todo_title: &str) -> Option<String> {
    let key = normalize_title_key(todo_title);
    if key.is_empty() {
        return None;
    }
    let mut best: Option<(usize, &PlanCheckboxItem)> = None;
    for item in items {
        let item_key = normalize_title_key(&item.title);
        if item_key.is_empty() {
            continue;
        }
        let score = if item_key == key {
            100
        } else if item_key.contains(&key) || key.contains(&item_key) {
            50 + common_prefix_len(&item_key, &key)
        } else {
            let prefix = common_prefix_len(&item_key, &key);
            if prefix >= 6 { prefix } else { 0 }
        };
        if score > 0 && best.as_ref().map(|(s, _)| score > *s).unwrap_or(true) {
            best = Some((score, item));
        }
    }
    best.map(|(_, item)| item.status.clone())
}

fn common_prefix_len(a: &str, b: &str) -> usize {
    a.chars().zip(b.chars()).take_while(|(x, y)| x == y).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_plan_checkbox_items_reads_markdown_checklist() {
        let plan = "# Plan\n\n- [x] 创建目录\n- [ ] 编写代码\n- [~] 测试中\n";
        let items = parse_plan_checkbox_items(plan);
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].status, "completed");
        assert_eq!(items[1].status, "pending");
        assert_eq!(items[2].status, "in_progress");
    }

    #[test]
    fn match_checkbox_status_fuzzy_matches_todo_title() {
        let items = parse_plan_checkbox_items("- [x] 创建游戏目录结构\n- [ ] 实现 HTML 页面结构\n");
        assert_eq!(
            match_checkbox_status(&items, "创建游戏目录结构"),
            Some("completed".into())
        );
        assert_eq!(
            match_checkbox_status(&items, "实现 HTML 页面结构"),
            Some("pending".into())
        );
        let done_items =
            parse_plan_checkbox_items("- [x] 实现 HTML 页面结构\n- [x] 实现 CSS 样式\n");
        assert_eq!(
            match_checkbox_status(&done_items, "实现 HTML + CSS + JS 游戏"),
            Some("completed".into())
        );
    }

    #[test]
    fn normalize_todo_status_maps_aliases() {
        assert_eq!(normalize_todo_status("done"), "completed");
        assert_eq!(normalize_todo_status("doing"), "in_progress");
        assert_eq!(normalize_todo_status("pending"), "pending");
    }

    #[test]
    fn todo_status_rank_orders_progress() {
        assert!(todo_status_rank("completed") > todo_status_rank("in_progress"));
        assert!(todo_status_rank("in_progress") > todo_status_rank("pending"));
    }
}
