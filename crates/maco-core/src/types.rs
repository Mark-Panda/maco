//! 跨 crate 共享的领域类型与 Run 状态常量。

use serde::{Deserialize, Serialize};

/// Run 生命周期状态：待执行。
pub const RUN_STATUS_PENDING: &str = "pending";
/// Run 正在执行中。
pub const RUN_STATUS_RUNNING: &str = "running";
/// Run 正常结束。
pub const RUN_STATUS_COMPLETED: &str = "completed";
/// Run 因错误终止。
pub const RUN_STATUS_FAILED: &str = "failed";
/// Run 被用户或系统取消。
pub const RUN_STATUS_CANCELLED: &str = "cancelled";
/// Run 等待用户输入（HITL / Elicitation）。
pub const RUN_STATUS_AWAITING_USER: &str = "awaiting_user";

/// `maco_session_meta.status` 枚举。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionMetaStatus {
    /// 正常使用中。
    Active,
    /// 已归档。
    Archived,
    /// 删除进行中。
    PendingDelete,
    /// 已删除。
    Deleted,
    /// adk 已创建但元数据写入失败（启动对账清理）。
    OrphanCreate,
}

impl SessionMetaStatus {
    /// 序列化为数据库/API 使用的 snake_case 字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Archived => "archived",
            Self::PendingDelete => "pending_delete",
            Self::Deleted => "deleted",
            Self::OrphanCreate => "orphan_create",
        }
    }

    /// 从数据库字符串解析状态。
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

/// Run 暂停时序列化进 `maco_runs.resume_context`，用于 HITL/Elicitation 恢复。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResumeContext {
    /// 结构版本号（当前为 1）。
    pub schema_version: u32,
    /// 暂停原因：`hitl` / `elicitation` 等。
    pub reason: String,
    /// 被暂停的 Run ID。
    pub parent_run_id: String,
    /// 待审批的工具调用（HITL）。
    #[serde(default)]
    pub pending_tool_call: Option<PendingToolCall>,
    /// 待响应的 Elicitation ID。
    #[serde(default)]
    pub pending_elicitation_id: Option<String>,
    /// 恢复时关联的用户消息 ID 列表。
    #[serde(default)]
    pub user_message_ids: Vec<String>,
    /// 恢复时是否跳过重放历史事件。
    #[serde(default = "default_true")]
    pub do_not_replay_events: bool,
}

fn default_true() -> bool {
    true
}

/// 待用户审批的工具调用快照。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingToolCall {
    /// 工具名称。
    pub name: String,
    /// 工具调用参数 JSON。
    pub args: serde_json::Value,
    /// adk 工具调用 ID。
    pub call_id: String,
}

/// 聊天 SSE 统一事件信封（`type` + `run_id` + 单调 `seq` + `payload`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SseEnvelope {
    /// 事件类型（如 `token` / `tool_call` / `elicitation_request`）。
    #[serde(rename = "type")]
    pub event_type: String,
    /// 所属 Run ID。
    pub run_id: String,
    /// 会话内单调递增序号。
    pub seq: u64,
    /// 事件载荷（结构因 type 而异）。
    pub payload: serde_json::Value,
}

/// 前端展示的待处理 MCP elicitation 摘要。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingElicitation {
    /// Elicitation 记录 ID。
    pub id: String,
    /// 请求类型：`form` / `url`。
    pub request_type: String,
    /// 展示给用户的提示文案。
    pub message: String,
    /// 表单 JSON Schema（form 类型）。
    pub schema: Option<serde_json::Value>,
    /// 跳转 URL（url 类型）。
    pub url: Option<String>,
    /// 发起请求的 MCP 服务名。
    pub mcp_server: String,
}

/// `GET /runs/:id` 返回的 Run 状态与挂起项。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunStatusResponse {
    /// Run ID。
    pub id: String,
    /// 所属会话 ID。
    pub session_id: String,
    /// 当前状态（见 `RUN_STATUS_*` 常量）。
    pub status: String,
    /// 最后 SSE 事件序号。
    pub last_seq: u64,
    /// 待用户审批的工具调用列表。
    pub pending_tools: Vec<PendingToolCall>,
    /// 待处理的 Elicitation 列表。
    pub pending_elicitations: Vec<PendingElicitation>,
    /// 失败时的错误信息。
    pub error_message: Option<String>,
}

/// 从 `resume_context` JSON 提取待审批工具列表。
pub fn pending_tools_from_resume(resume_context: Option<&str>) -> Vec<PendingToolCall> {
    parse_resume_context(resume_context)
        .and_then(|ctx| ctx.pending_tool_call)
        .into_iter()
        .collect()
}

/// 从 `resume_context` 提取待处理 elicitation ID（详情需再查 DB）。
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

/// `GET /memory/search` 响应体。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySearchResponse {
    /// 检索模式（当前为 `keyword`）。
    pub search_mode: String,
    /// 命中结果列表。
    pub results: Vec<MemorySearchHit>,
}

/// 单条 Memory 检索命中。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySearchHit {
    /// 记忆文本内容。
    pub content: String,
    /// 相关度分数（keyword 模式可能为 null）。
    pub score: Option<f64>,
}

/// `GET /memory` 列表中的单条记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryListItem {
    /// adk memory 行 ID。
    pub id: i64,
    /// 记忆文本。
    pub content: String,
    /// 作者（`user` / `agent` 等）。
    pub author: String,
    /// 写入时间（RFC3339）。
    pub timestamp: String,
    /// 来源会话 ID。
    pub session_id: String,
}

/// `GET /memory` 响应体。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryListResponse {
    /// Memory 条目列表。
    pub items: Vec<MemoryListItem>,
}
