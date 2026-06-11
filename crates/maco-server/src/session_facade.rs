use std::collections::HashMap;
use std::sync::Arc;

use adk_core::{Content, Event, EventActions, Part};
use adk_memory::{MemoryEntry, SearchRequest};
use chrono::Utc;
use maco_storage::memory_admin;
use adk_session::{CreateRequest, DeleteRequest, GetRequest, ListRequest};
use maco_core::{
    MacoError, MacoResult, MemoryListItem, MemoryListResponse, MemorySearchResponse, APP_NAME,
    USER_ID,
};
use maco_db::{ModelRecord, ModelRepo, SessionMetaRecord, SessionMetaRepo};
use maco_storage::AdkStorage;
use serde_json::json;

pub struct SessionFacade {
    adk: Arc<AdkStorage>,
    meta: SessionMetaRepo,
}

impl SessionFacade {
    pub fn new(adk: Arc<AdkStorage>, meta: SessionMetaRepo) -> Self {
        Self { adk, meta }
    }

    pub async fn create_session(
        &self,
        title: Option<String>,
        model_id: Option<String>,
    ) -> MacoResult<SessionMetaRecord> {
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
        let rec = SessionMetaRepo::new_record(session_id.clone(), title, model_id);
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
                let rec = SessionMetaRepo::new_record(sid.clone(), None, None);
                let _ = self.meta.insert(&rec).await;
                metas.push(rec);
            }
        }
        metas.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(metas)
    }

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

    pub async fn memory_delete(&self, query: &str) -> MacoResult<u64> {
        self.adk
            .memory
            .delete_entries(APP_NAME, USER_ID, query)
            .await
            .map_err(|e| MacoError::Adk(e.to_string()))
    }

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
