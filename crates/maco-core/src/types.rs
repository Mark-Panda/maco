use serde::{Deserialize, Serialize};

pub const RUN_STATUS_PENDING: &str = "pending";
pub const RUN_STATUS_RUNNING: &str = "running";
pub const RUN_STATUS_COMPLETED: &str = "completed";
pub const RUN_STATUS_FAILED: &str = "failed";
pub const RUN_STATUS_CANCELLED: &str = "cancelled";
pub const RUN_STATUS_AWAITING_USER: &str = "awaiting_user";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionMetaStatus {
    Active,
    Archived,
    PendingDelete,
    Deleted,
    OrphanCreate,
}

impl SessionMetaStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Archived => "archived",
            Self::PendingDelete => "pending_delete",
            Self::Deleted => "deleted",
            Self::OrphanCreate => "orphan_create",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "active" => Some(Self::Active),
            "archived" => Some(Self::Archived),
            "pending_delete" => Some(Self::PendingDelete),
            "deleted" => Some(Self::Deleted),
            "orphan_create" => Some(Self::OrphanCreate),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResumeContext {
    pub schema_version: u32,
    pub reason: String,
    pub parent_run_id: String,
    #[serde(default)]
    pub pending_tool_call: Option<PendingToolCall>,
    #[serde(default)]
    pub pending_elicitation_id: Option<String>,
    #[serde(default)]
    pub user_message_ids: Vec<String>,
    #[serde(default = "default_true")]
    pub do_not_replay_events: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingToolCall {
    pub name: String,
    pub args: serde_json::Value,
    pub call_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SseEnvelope {
    #[serde(rename = "type")]
    pub event_type: String,
    pub run_id: String,
    pub seq: u64,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingElicitation {
    pub id: String,
    pub request_type: String,
    pub message: String,
    pub schema: Option<serde_json::Value>,
    pub url: Option<String>,
    pub mcp_server: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunStatusResponse {
    pub id: String,
    pub session_id: String,
    pub status: String,
    pub last_seq: u64,
    pub pending_tools: Vec<PendingToolCall>,
    pub pending_elicitations: Vec<PendingElicitation>,
    pub error_message: Option<String>,
}

pub fn pending_tools_from_resume(resume_context: Option<&str>) -> Vec<PendingToolCall> {
    parse_resume_context(resume_context)
        .and_then(|ctx| ctx.pending_tool_call)
        .into_iter()
        .collect()
}

pub fn pending_elicitations_from_resume(resume_context: Option<&str>) -> Vec<PendingElicitation> {
    let Some(ctx) = parse_resume_context(resume_context) else {
        return vec![];
    };
    let Some(id) = ctx.pending_elicitation_id else {
        return vec![];
    };
    vec![PendingElicitation {
        id,
        request_type: String::new(),
        message: String::new(),
        schema: None,
        url: None,
        mcp_server: String::new(),
    }]
}

fn parse_resume_context(resume_context: Option<&str>) -> Option<ResumeContext> {
    let raw = resume_context?;
    serde_json::from_str(raw).ok()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySearchResponse {
    pub search_mode: String,
    pub results: Vec<MemorySearchHit>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySearchHit {
    pub content: String,
    pub score: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryListItem {
    pub id: i64,
    pub content: String,
    pub author: String,
    pub timestamp: String,
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryListResponse {
    pub items: Vec<MemoryListItem>,
}
