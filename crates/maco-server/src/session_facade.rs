//! Session 门面：协调 adk Session/Memory 与 `maco_session_meta` 业务元数据。

use std::collections::HashMap;
use std::sync::Arc;

use adk_core::{Content, Event, EventActions, Part};
use adk_memory::{MemoryEntry, SearchRequest};
use chrono::Utc;
use maco_storage::memory_admin;
use adk_session::{CreateRequest, DeleteRequest, GetRequest, ListRequest};
use maco_core::{
    resolve_project_root, ChatMessage, MacoError, MacoResult, MemoryListItem, MemoryListResponse,
    MemorySearchResponse, SessionMessagesResponse, APP_NAME, USER_ID,
};
use maco_db::{ModelRecord, ModelRepo, SessionMetaRecord, SessionMetaRepo};
use maco_storage::AdkStorage;
use serde_json::json;

/// 对外统一的 Session + Memory 操作入口（HTTP 层主要依赖此类型）。
pub struct SessionFacade {
    adk: Arc<AdkStorage>,
    meta: SessionMetaRepo,
}

impl SessionFacade {
    /// 构造门面，注入 adk 存储与元数据仓库。
    pub fn new(adk: Arc<AdkStorage>, meta: SessionMetaRepo) -> Self {
        Self { adk, meta }
    }

    /// 在 adk 与 `maco_session_meta` 中同时创建会话；失败时回滚 adk 侧。
    pub async fn create_session(
        &self,
        title: Option<String>,
        model_id: Option<String>,
        project_root: Option<String>,
    ) -> MacoResult<SessionMetaRecord> {
        let project_root = resolve_project_root(project_root.as_deref())?
            .map(|p| p.to_string_lossy().into_owned());
        let mut state = std::collections::HashMap::new();
        if let Some(ref mid) = model_id {
            state.insert("user:model".into(), json!(mid));
        }
        let session = self
            .adk
            .session
            .create(CreateRequest {
                app_name: APP_NAME.into(),
                user_id: USER_ID.into(),
                session_id: None,
                state,
            })
            .await
            .map_err(|e| MacoError::Adk(e.to_string()))?;
        let session_id = session.id().to_string();
        let rec = SessionMetaRepo::new_record(session_id.clone(), title, model_id, project_root);
        if let Err(e) = self.meta.insert(&rec).await {
            let _ = self
                .adk
                .session
                .delete(DeleteRequest {
                    app_name: APP_NAME.into(),
                    user_id: USER_ID.into(),
                    session_id,
                })
                .await;
            return Err(e);
        }
        Ok(rec)
    }

    /// 列出会话：与 adk 对齐，并为缺失元数据的 session 自动补建记录。
    pub async fn list_sessions(&self) -> MacoResult<Vec<SessionMetaRecord>> {
        let adk_sessions = self
            .adk
            .session
            .list(ListRequest {
                app_name: APP_NAME.into(),
                user_id: USER_ID.into(),
                limit: None,
                offset: None,
            })
            .await
            .map_err(|e| MacoError::Adk(e.to_string()))?;
        let ids: Vec<String> = adk_sessions.iter().map(|s| s.id().to_string()).collect();
        let mut metas = self.meta.list_by_ids(&ids).await?;
        let known: std::collections::HashSet<_> = metas.iter().map(|m| m.session_id.clone()).collect();
        for s in adk_sessions {
            let sid = s.id().to_string();
            if !known.contains(&sid) {
                let rec = SessionMetaRepo::new_record(sid.clone(), None, None, None);
                let _ = self.meta.insert(&rec).await;
                metas.push(rec);
            }
        }
        metas.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(metas)
    }

    /// 软删 → 清理 memory 引用 → 删除 adk session → 标记 deleted。
    pub async fn delete_session(&self, session_id: &str) -> MacoResult<()> {
        self.meta.update_status(session_id, "pending_delete").await?;
        self.adk
            .memory
            .add_session(APP_NAME, USER_ID, session_id, vec![])
            .await
            .ok();
        self.adk
            .session
            .delete(DeleteRequest {
                app_name: APP_NAME.into(),
                user_id: USER_ID.into(),
                session_id: session_id.to_string(),
            })
            .await
            .map_err(|e| MacoError::Adk(e.to_string()))?;
        self.meta.update_status(session_id, "deleted").await?;
        Ok(())
    }

    /// 绑定或清除会话的项目根目录。
    pub async fn set_project_root(
        &self,
        session_id: &str,
        project_root: Option<&str>,
    ) -> MacoResult<()> {
        let normalized = resolve_project_root(project_root)?
            .map(|p| p.to_string_lossy().into_owned());
        self.meta
            .update_project_root(session_id, normalized.as_deref())
            .await
    }

    /// 更新会话绑定模型（元数据 + adk state_delta `user:model`）。
    pub async fn set_model(&self, session_id: &str, model_id: &str) -> MacoResult<()> {
        self.meta
            .update_title_model(session_id, None, Some(model_id))
            .await?;

        let mut state_delta = HashMap::new();
        state_delta.insert("user:model".into(), json!(model_id));
        let mut event = Event::new("maco-set-model");
        event.author = "maco".into();
        event.actions = EventActions {
            state_delta,
            ..Default::default()
        };
        self.adk
            .session
            .append_event(session_id, event)
            .await
            .map_err(|e| MacoError::Adk(e.to_string()))?;
        Ok(())
    }

    /// 解析本次 Run 使用的模型：请求覆盖 > session state > meta > 默认模型。
    pub async fn resolve_model(
        &self,
        models: &ModelRepo,
        session_id: &str,
        override_id: Option<&str>,
    ) -> MacoResult<ModelRecord> {
        if let Some(id) = override_id {
            return models
                .get(id)
                .await?
                .ok_or_else(|| MacoError::not_found("model"));
        }

        let session = self
            .adk
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

        if let Some(mid) = session
            .state()
            .get("user:model")
            .and_then(|v| v.as_str().map(str::to_string))
        {
            if let Some(model) = models.get(&mid).await? {
                return Ok(model);
            }
        }

        if let Some(meta) = self.meta.get(session_id).await? {
            if let Some(mid) = meta.model_id {
                if let Some(model) = models.get(&mid).await? {
                    return Ok(model);
                }
            }
        }

        models
            .get_default()
            .await?
            .ok_or_else(|| MacoError::not_found("default model"))
    }

    /// 启动时修复孤儿/半删会话（`orphan_create` / `pending_delete`）。
    pub async fn reconcile(&self) -> MacoResult<()> {
        let orphans = self.meta.list_orphans().await?;
        for o in orphans {
            match o.status.as_str() {
                "orphan_create" => {
                    let _ = self
                        .adk
                        .session
                        .delete(DeleteRequest {
                            app_name: APP_NAME.into(),
                            user_id: USER_ID.into(),
                            session_id: o.session_id.clone(),
                        })
                        .await;
                    self.meta.update_status(&o.session_id, "deleted").await?;
                }
                "pending_delete" => {
                    let _ = self.delete_session(&o.session_id).await;
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// 分页列出 adk memory 条目（管理 API）。
    pub async fn memory_list(&self, limit: usize) -> MacoResult<MemoryListResponse> {
        let rows = memory_admin::list_from_pool(self.adk.memory_pool(), limit).await?;
        Ok(MemoryListResponse {
            items: rows
                .into_iter()
                .map(|r| MemoryListItem {
                    id: r.id,
                    content: r.content,
                    author: r.author,
                    timestamp: r.timestamp,
                    session_id: r.session_id,
                })
                .collect(),
        })
    }

    /// 向全局 memory 追加一条用户文本。
    pub async fn memory_add(&self, content: &str) -> MacoResult<()> {
        let entry = MemoryEntry {
            content: Content {
                role: "user".into(),
                parts: vec![Part::Text {
                    text: content.to_string(),
                }],
            },
            author: "user".into(),
            timestamp: Utc::now(),
        };
        self.adk
            .memory
            .add_entry(APP_NAME, USER_ID, entry)
            .await
            .map_err(|e| MacoError::Adk(e.to_string()))
    }

    /// 按关键词删除 memory 条目，返回删除数量。
    pub async fn memory_delete(&self, query: &str) -> MacoResult<u64> {
        self.adk
            .memory
            .delete_entries(APP_NAME, USER_ID, query)
            .await
            .map_err(|e| MacoError::Adk(e.to_string()))
    }

    /// 从 adk session events 提取用户/助手文本消息（供前端恢复对话）。
    pub async fn session_messages(&self, session_id: &str) -> MacoResult<SessionMessagesResponse> {
        let session = self
            .adk
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

        let mut messages = Vec::new();
        for event in session.events().all() {
            let Some(content) = event.llm_response.content.as_ref() else {
                continue;
            };
            let text = message_text_from_content(content);
            if text.is_empty() {
                continue;
            }
            let role = if event.author == "user" {
                "user"
            } else {
                "assistant"
            };
            messages.push(ChatMessage {
                role: role.into(),
                content: text,
            });
        }
        Ok(SessionMessagesResponse { messages })
    }

    /// 关键词检索 memory（当前为 adk 内置 keyword 模式）。
    pub async fn memory_search(&self, query: &str) -> MacoResult<MemorySearchResponse> {
        let resp = self
            .adk
            .memory
            .search(SearchRequest {
                app_name: APP_NAME.into(),
                user_id: USER_ID.into(),
                query: query.into(),
                limit: Some(5),
                min_score: None,
                project_id: None,
            })
            .await
            .map_err(|e| MacoError::Adk(e.to_string()))?;
        Ok(MemorySearchResponse {
            search_mode: "keyword".into(),
            results: resp
                .memories
                .into_iter()
                .map(|m| maco_core::MemorySearchHit {
                    content: m
                        .content
                        .parts
                        .iter()
                        .filter_map(|p| match p {
                            adk_core::Part::Text { text } => Some(text.clone()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join(" "),
                    score: None,
                })
                .collect(),
        })
    }
}

fn message_text_from_content(content: &Content) -> String {
    let has_tool_call = content
        .parts
        .iter()
        .any(|p| matches!(p, Part::FunctionCall { .. }));
    if has_tool_call {
        return String::new();
    }
    content
        .parts
        .iter()
        .filter_map(|p| match p {
            Part::Text { text } => Some(text.as_str()),
            Part::Thinking { .. } => None,
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
}
