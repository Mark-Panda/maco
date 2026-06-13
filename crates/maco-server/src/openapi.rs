//! utoipa OpenAPI 文档定义与 Swagger UI 挂载用的 schema。

#![allow(dead_code)]

use utoipa::OpenApi;

/// `GET /health` 响应体。
#[derive(utoipa::ToSchema, serde::Serialize)]
pub struct HealthResponse {
    /// `ok` 或 `degraded`。
    pub overall: String,
    /// 业务数据库连通状态。
    pub db: DependencyHealthDoc,
    /// adk session 数据库连通状态。
    pub session: DependencyHealthDoc,
    /// adk memory 数据库连通状态。
    pub memory: DependencyHealthDoc,
    /// MCP 池状态摘要。
    pub mcp: Vec<String>,
    /// MCP 详细健康状态。
    pub mcp_status: McpPoolHealthDoc,
    /// 服务绑定地址。
    pub bind: String,
    /// Agent scratch 临时目录根路径。
    pub tmp_dir: String,
}

/// 单个依赖的健康状态。
#[derive(utoipa::ToSchema, serde::Serialize)]
pub struct DependencyHealthDoc {
    /// `ok` 或 `failed`。
    pub status: String,
    /// 失败原因。
    pub error: Option<String>,
}

/// 单个 MCP 服务的运行态状态。
#[derive(utoipa::ToSchema, serde::Serialize)]
pub struct McpServerStatusDoc {
    /// MCP 服务名。
    pub name: String,
    /// `stdio` 或 `sse`。
    pub transport: String,
    /// `connected` 或 `failed`。
    pub status: String,
    /// 失败原因。
    pub error: Option<String>,
}

/// MCP pool 聚合健康状态。
#[derive(utoipa::ToSchema, serde::Serialize)]
pub struct McpPoolHealthDoc {
    /// `ok` 或 `degraded`。
    pub overall: String,
    /// 各 MCP server 状态。
    pub servers: Vec<McpServerStatusDoc>,
}

/// 会话元数据（列表/详情）。
#[derive(utoipa::ToSchema, serde::Serialize)]
pub struct SessionMetaDoc {
    /// 会话 ID（与 adk session 一致）。
    pub session_id: String,
    /// 显示标题。
    pub title: Option<String>,
    /// 绑定模型 ID。
    pub model_id: Option<String>,
    /// 绑定的本地项目根目录。
    pub project_root: Option<String>,
    /// Agent 权限模式（`request_approval` / `auto_approve` / `full_access`）。
    pub permission_mode: String,
    /// 是否强制 Git worktree。
    pub git_worktree_enabled: i64,
    /// worktree 分支前缀。
    pub git_branch_prefix: String,
    /// worktree 检出路径。
    pub git_worktree_path: Option<String>,
    /// worktree 分支名。
    pub git_worktree_branch: Option<String>,
    /// worktree 状态（`disabled` / `no_project` / `not_git_repo` / `git_unavailable` / `pending` / `active`）。
    pub git_worktree_status: String,
    /// 生命周期状态（`active` / `deleted` 等）。
    pub status: String,
}

/// `POST /sessions` 请求体。
#[derive(utoipa::ToSchema, serde::Deserialize)]
pub struct CreateSessionDoc {
    /// 会话标题。
    pub title: Option<String>,
    /// 初始模型 ID。
    pub model_id: Option<String>,
    /// 绑定的本地项目根目录。
    pub project_root: Option<String>,
    /// Agent 权限模式，默认 `request_approval`。
    pub permission_mode: Option<String>,
    /// 是否强制 Git worktree，默认 `true`。
    pub git_worktree_enabled: Option<bool>,
    /// worktree 分支前缀，默认 `maco/agent`。
    pub git_branch_prefix: Option<String>,
}

/// `POST /chat` 请求体。
#[derive(utoipa::ToSchema, serde::Deserialize)]
pub struct ChatRequestDoc {
    /// 目标会话 ID。
    pub session_id: String,
    /// 用户消息文本。
    pub message: String,
    /// 覆盖使用的模型 ID（可选）。
    pub model_id: Option<String>,
}

/// 模型配置视图（api_key 已脱敏）。
#[derive(utoipa::ToSchema, serde::Serialize)]
pub struct ModelViewDoc {
    /// 模型配置 ID。
    pub id: String,
    /// 显示名称。
    pub name: String,
    /// 提供商：`openai` / `anthropic` / `gemini` / `openrouter`。
    pub provider: String,
    /// 上游模型标识。
    pub model_id: String,
    /// 自定义 API Base URL。
    pub base_url: Option<String>,
    /// 是否为默认模型。
    pub is_default: bool,
    /// 是否已配置 API Key。
    pub has_api_key: bool,
}

/// `POST/PATCH /models` 请求体。
#[derive(utoipa::ToSchema, serde::Deserialize)]
pub struct ModelUpsertDoc {
    /// 显示名称。
    pub name: String,
    /// 提供商。
    pub provider: String,
    /// 上游模型标识。
    pub model_id: String,
    /// 自定义 Base URL。
    pub base_url: Option<String>,
    /// 内联 API Key（可选）。
    pub api_key: Option<String>,
    /// 环境变量名（API Key 兜底）。
    pub api_key_env: Option<String>,
    /// 是否设为默认模型。
    pub is_default: bool,
}

/// Memory 检索 query 参数。
#[derive(utoipa::ToSchema, serde::Deserialize)]
pub struct MemorySearchQueryDoc {
    /// 搜索关键词。
    pub q: String,
}

/// `POST /system/pick-directory` 响应体。
#[derive(utoipa::ToSchema, serde::Serialize)]
pub struct PickDirectoryDoc {
    /// 用户是否取消了选择对话框。
    pub cancelled: bool,
    /// 选中的文件夹绝对路径（未取消时存在）。
    pub path: Option<String>,
}

/// `GET /guardrail/status` 响应体。
#[derive(utoipa::ToSchema, serde::Serialize)]
pub struct GuardrailStatusDoc {
    /// 是否启用 PII 脱敏。
    pub pii_enabled: bool,
    /// 日志脱敏级别（如 `basic`）。
    pub log_redact: String,
    /// worktree 模式下是否拦截 bash / MCP 访问主仓库路径。
    pub worktree_path_guard: bool,
}

/// worktree 路径守卫开关。
#[derive(utoipa::ToSchema, serde::Serialize, serde::Deserialize)]
pub struct WorktreePathGuardDoc {
    /// 是否启用路径强制。
    pub enabled: bool,
}

/// 活跃 Run 查询结果（内存流优先，否则 DB 回退）。
#[derive(utoipa::ToSchema, serde::Serialize)]
pub struct ActiveRunDoc {
    /// 会话 ID。
    pub session_id: String,
    /// 活跃 Run ID；无活跃 Run 时为 null。
    pub run_id: Option<String>,
    /// Run 状态（仅 `source=db` 时可能存在）。
    pub status: Option<String>,
    /// 数据来源：`stream`（内存）或 `db`。
    pub source: Option<String>,
}

/// Run 状态响应摘要。
#[derive(utoipa::ToSchema, serde::Serialize)]
pub struct RunStatusDoc {
    /// Run ID。
    pub id: String,
    /// 会话 ID。
    pub session_id: String,
    /// 状态：`running` / `awaiting_user` / `completed` 等。
    pub status: String,
    /// 最后事件序号。
    pub last_seq: u64,
}

/// Run SSE envelope。`type` 与 `event_type` 语义相同，前端兼容读取两者。
#[derive(utoipa::ToSchema, serde::Serialize)]
pub struct SseEnvelopeDoc {
    /// 事件类型，如 `text` / `tool_call` / `done` / `stream_gap` / `stream_ended` / `stream_unavailable`。
    pub event_type: String,
    /// Run ID。
    pub run_id: String,
    /// 单 Run 内递增事件序号；客户端可用它检测 replay 缺口与去重。
    pub seq: u64,
    /// 事件 payload；普通事件按各自类型解释，replay marker 见 `SseReplayMarkerPayloadDoc`。
    pub payload: serde_json::Value,
}

/// SSE replay marker payload，用于 `stream_gap` / `stream_ended` / `stream_unavailable`。
#[derive(utoipa::ToSchema, serde::Serialize)]
pub struct SseReplayMarkerPayloadDoc {
    /// Run 当前或最终状态。`stream_gap` 事件可能为空。
    pub status: Option<String>,
    /// Run 已记录的最后 SSE 序号，表示历史上是否存在事件，而不是本次请求一定有新事件。
    pub last_seq: Option<i64>,
    /// 本次响应实际回放到的最后序号；为 null 表示没有可回放事件。
    pub last_replayed_seq: Option<u64>,
    /// 是否存在已知缺口。客户端应提示用户该 Run 历史可能不完整，并可重新加载会话最终消息。
    pub gap: bool,
    /// 该 Run 历史上是否存在可回放事件；不表示本次请求还有未读事件。
    pub replay_available: Option<bool>,
    /// Run 是否仍处于可继续运行状态。`stream_unavailable` + `live_stream=true` 表示服务端当前无法接入实时内存流。
    pub live_stream: Option<bool>,
    /// `stream_gap` 中回显客户端传入的 `after_seq`。
    pub after_seq: Option<u64>,
    /// 面向调试/展示的说明文本。
    pub message: Option<String>,
}

/// ReAct 计划记录。
#[derive(utoipa::ToSchema, serde::Serialize)]
pub struct PlanDoc {
    /// 会话 ID。
    pub session_id: String,
    /// 计划正文。
    pub content: String,
    /// 乐观锁版本号。
    pub version: i64,
}

/// 更新 ReAct 计划请求。
#[derive(utoipa::ToSchema, serde::Deserialize)]
pub struct PutPlanDoc {
    /// 计划 Markdown/文本内容。
    pub content: String,
    /// 乐观锁版本号。
    pub version: Option<i64>,
}

/// Todo 记录。
#[derive(utoipa::ToSchema, serde::Serialize)]
pub struct TodoDoc {
    /// 会话 ID。
    pub session_id: String,
    /// 稳定任务 key。
    pub task_key: String,
    /// 展示标题。
    pub title: String,
    /// 状态。
    pub status: String,
}

/// 更新 Todo 状态请求。
#[derive(utoipa::ToSchema, serde::Deserialize)]
pub struct PatchTodoDoc {
    /// 新状态。
    pub status: String,
}

/// 子 Agent 执行审计记录。
#[derive(utoipa::ToSchema, serde::Serialize)]
pub struct SubAgentRunDoc {
    /// 记录 ID。
    pub id: String,
    /// 会话 ID。
    pub session_id: String,
    /// 父 Run ID。
    pub parent_run_id: String,
    /// Todo task key。
    pub task_key: String,
    /// worker agent 名称。
    pub worker_agent: String,
    /// 工具 profile。
    pub tools_profile: String,
    /// running / completed / failed / cancelled / timeout。
    pub status: String,
    /// 子任务 instruction。
    pub instruction: String,
    /// 完成摘要。
    pub summary: Option<String>,
    /// 错误信息。
    pub error: Option<String>,
}

/// 附件预览响应。
#[derive(utoipa::ToSchema, serde::Serialize)]
pub struct ArtifactPreviewDoc {
    /// 附件 ID。
    pub id: String,
    /// 文件名。
    pub filename: String,
    /// MIME 类型。
    pub mime_type: String,
    /// 是否可预览。
    pub previewable: bool,
    /// `image` / `text` / `binary`。
    pub kind: String,
    /// 文本预览内容。
    pub content: Option<String>,
    /// 是否被截断。
    pub truncated: bool,
}

/// HITL 恢复请求体。
#[derive(utoipa::ToSchema, serde::Deserialize)]
pub struct ResumeRunDoc {
    /// 是否批准挂起工具。
    pub approved: bool,
    /// 用户备注。
    pub note: Option<String>,
    /// 覆盖模型 ID。
    pub model_id: Option<String>,
}

/// Elicitation 响应请求体。
#[derive(utoipa::ToSchema, serde::Deserialize)]
pub struct ElicitationRespondDoc {
    /// `accept` / `decline` / `cancel`。
    pub action: String,
    /// 用户提交内容（accept 时）。
    pub content: Option<serde_json::Value>,
}

/// MCP 服务配置。内置 `filesystem` 由 maco 按会话启动子进程，不在全局池中重载。
#[derive(utoipa::ToSchema, serde::Serialize, serde::Deserialize)]
pub struct McpServerDoc {
    /// 配置 ID。
    pub id: String,
    /// 服务名（内置 `filesystem` 不可删除）。
    pub name: String,
    /// `stdio` 或 `sse`。
    pub transport: String,
    /// stdio 命令。
    pub command: Option<String>,
    /// SSE URL。
    pub url: Option<String>,
    /// 是否启用。
    pub enabled: i64,
}

/// 后台任务记录摘要。
#[derive(utoipa::ToSchema, serde::Serialize)]
pub struct JobDoc {
    /// 任务 ID。
    pub id: String,
    /// 显示名称。
    pub name: String,
    /// 任务类型。
    pub job_type: String,
    /// 调度周期。
    pub schedule: Option<String>,
    /// 状态。
    pub status: String,
}

/// API Token 创建请求。
#[derive(utoipa::ToSchema, serde::Deserialize)]
pub struct CreateTokenDoc {
    /// Token 名称。
    pub name: String,
    /// 授权 scope 列表。
    pub scopes: Option<Vec<String>>,
    /// 过期时间（RFC3339）。
    pub expires_at: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/health",
    tag = "system",
    responses(
        (status = 200, description = "服务健康检查通过", body = HealthResponse),
        (status = 503, description = "服务依赖降级", body = HealthResponse)
    )
)]
fn health_doc() {}

#[utoipa::path(
    get,
    path = "/api/guardrail/status",
    tag = "system",
    responses((status = 200, description = "治理/脱敏配置", body = GuardrailStatusDoc))
)]
fn guardrail_status_doc() {}

#[utoipa::path(
    post,
    path = "/api/system/pick-directory",
    tag = "system",
    responses((status = 200, description = "本机文件夹选择结果", body = PickDirectoryDoc))
)]
fn pick_directory_doc() {}

#[utoipa::path(
    get,
    path = "/api/sessions",
    tag = "sessions",
    responses((status = 200, description = "列出所有会话", body = [SessionMetaDoc]))
)]
fn list_sessions_doc() {}

#[utoipa::path(
    post,
    path = "/api/sessions",
    tag = "sessions",
    request_body = CreateSessionDoc,
    responses((status = 200, description = "创建会话", body = SessionMetaDoc))
)]
fn create_session_doc() {}

#[utoipa::path(
    get,
    path = "/api/sessions/{id}/export",
    tag = "sessions",
    params(("id" = String, Path, description = "会话 ID")),
    responses((status = 200, description = "导出 Markdown", content_type = "text/markdown"))
)]
fn export_session_doc() {}

/// 聊天消息。
#[derive(utoipa::ToSchema, serde::Serialize)]
pub struct ChatMessageDoc {
    /// `user` 或 `assistant`。
    pub role: String,
    /// 消息正文。
    pub content: String,
}

#[utoipa::path(
    get,
    path = "/api/sessions/{id}/messages",
    tag = "sessions",
    params(("id" = String, Path, description = "会话 ID")),
    responses((status = 200, description = "会话历史消息", body = [ChatMessageDoc]))
)]
fn session_messages_doc() {}

#[utoipa::path(
    get,
    path = "/api/sessions/{id}/plan",
    tag = "sessions",
    params(("id" = String, Path, description = "会话 ID")),
    responses((status = 200, description = "获取 ReAct 计划", body = PlanDoc))
)]
fn get_plan_doc() {}

#[utoipa::path(
    put,
    path = "/api/sessions/{id}/plan",
    tag = "sessions",
    params(("id" = String, Path, description = "会话 ID")),
    request_body = PutPlanDoc,
    responses((status = 200, description = "创建或更新 ReAct 计划", body = PlanDoc))
)]
fn put_plan_doc() {}

#[utoipa::path(
    get,
    path = "/api/sessions/{id}/todos",
    tag = "sessions",
    params(("id" = String, Path, description = "会话 ID")),
    responses((status = 200, description = "列出 Todo", body = [TodoDoc]))
)]
fn list_todos_doc() {}

#[utoipa::path(
    patch,
    path = "/api/sessions/{id}/todos/{task_key}",
    tag = "sessions",
    params(
        ("id" = String, Path, description = "会话 ID"),
        ("task_key" = String, Path, description = "Todo task key"),
    ),
    request_body = PatchTodoDoc,
    responses((status = 200, description = "更新 Todo 状态", body = TodoDoc))
)]
fn patch_todo_doc() {}

#[utoipa::path(
    post,
    path = "/api/mcp/servers",
    tag = "mcp",
    responses((status = 200, description = "创建 MCP 服务", body = McpServerDoc))
)]
fn create_mcp_doc() {}

#[utoipa::path(
    patch,
    path = "/api/mcp/servers/{id}",
    tag = "mcp",
    params(("id" = String, Path, description = "MCP 配置 ID")),
    responses((status = 200, description = "更新 MCP 服务", body = McpServerDoc))
)]
fn update_mcp_doc() {}

#[utoipa::path(
    delete,
    path = "/api/mcp/servers/{id}",
    tag = "mcp",
    params(("id" = String, Path, description = "MCP 配置 ID")),
    responses((status = 204, description = "删除 MCP 服务"))
)]
fn delete_mcp_doc() {}

#[utoipa::path(
    get,
    path = "/api/sessions/{id}/runs/active",
    tag = "runs",
    params(("id" = String, Path, description = "会话 ID")),
    responses((status = 200, description = "当前活跃 Run（stream 优先，db 回退）", body = ActiveRunDoc))
)]
fn active_run_doc() {}

#[utoipa::path(
    post,
    path = "/api/sessions/{id}/worktree/provision",
    tag = "sessions",
    params(("id" = String, Path, description = "会话 ID")),
    responses((status = 200, description = "手动 provision worktree 并返回会话元数据", body = SessionMetaDoc))
)]
fn provision_worktree_doc() {}

#[utoipa::path(
    get,
    path = "/api/sessions/{id}/runs/{run_id}",
    tag = "runs",
    params(
        ("id" = String, Path, description = "会话 ID"),
        ("run_id" = String, Path, description = "Run ID"),
    ),
    responses((status = 200, description = "Run 状态", body = RunStatusDoc))
)]
fn get_run_doc() {}

#[utoipa::path(
    get,
    path = "/api/sessions/{id}/runs/{run_id}/stream",
    tag = "runs",
    params(
        ("id" = String, Path, description = "会话 ID"),
        ("run_id" = String, Path, description = "Run ID"),
        ("after_seq" = Option<u64>, Query, description = "已收到的最后 SSE seq；用于短断线回放"),
    ),
    responses((
        status = 200,
        description = "SSE 重连订阅。除常规 Run 事件外，可能返回 `stream_gap`（历史回放存在缺口）、`stream_ended`（Run 已结束并给出最终 replay 元信息）、`stream_unavailable`（Run 尚未结束但当前实时内存流不可用）。`replay_available` 表示该 Run 历史上存在可回放事件，不表示本次请求一定还有新事件。",
        body = SseEnvelopeDoc,
        content_type = "text/event-stream"
    ))
)]
fn stream_run_doc() {}

#[utoipa::path(
    post,
    path = "/api/sessions/{id}/runs/{run_id}/resume",
    tag = "runs",
    request_body = ResumeRunDoc,
    responses((status = 200, description = "HITL 恢复后 SSE 流", content_type = "text/event-stream"))
)]
fn resume_run_doc() {}

#[utoipa::path(
    get,
    path = "/api/sessions/{id}/sub-agent-runs",
    tag = "runs",
    params(("id" = String, Path, description = "会话 ID")),
    responses((status = 200, description = "子 Agent spawn 审计列表", body = [SubAgentRunDoc]))
)]
fn list_sub_agent_runs_doc() {}

#[utoipa::path(
    get,
    path = "/api/sessions/{id}/sub-agent-runs/{sub_run_id}",
    tag = "runs",
    params(
        ("id" = String, Path, description = "会话 ID"),
        ("sub_run_id" = String, Path, description = "子 Agent Run ID"),
    ),
    responses((status = 200, description = "子 Agent spawn 审计详情", body = SubAgentRunDoc))
)]
fn get_sub_agent_run_doc() {}

#[utoipa::path(
    post,
    path = "/api/sessions/{id}/runs/{run_id}/sub-agents/{task_key}/cancel",
    tag = "runs",
    params(
        ("id" = String, Path, description = "会话 ID"),
        ("run_id" = String, Path, description = "父 Run ID"),
        ("task_key" = String, Path, description = "子 Agent task key"),
    ),
    responses((status = 200, description = "取消活跃子 Agent"))
)]
fn cancel_sub_agent_doc() {}

#[utoipa::path(
    get,
    path = "/api/sessions/{id}/elicitation/pending",
    tag = "elicitation",
    params(("id" = String, Path, description = "会话 ID")),
    responses((status = 200, description = "待处理 Elicitation 列表"))
)]
fn list_elicitation_doc() {}

#[utoipa::path(
    post,
    path = "/api/elicitation/{id}/respond",
    tag = "elicitation",
    request_body = ElicitationRespondDoc,
    responses((status = 200, description = "提交 Elicitation 响应"))
)]
fn respond_elicitation_doc() {}

#[utoipa::path(
    post,
    path = "/api/chat",
    tag = "chat",
    request_body = ChatRequestDoc,
    responses((
        status = 200,
        description = "SSE 流式返回 `SseEnvelope` 事件；断线重连语义同 Run stream endpoint。",
        body = SseEnvelopeDoc,
        content_type = "text/event-stream"
    ))
)]
fn chat_doc() {}

#[utoipa::path(
    post,
    path = "/api/chat/{session_id}/interrupt",
    tag = "chat",
    params(("session_id" = String, Path, description = "会话 ID")),
    responses((status = 200, description = "中断活跃 Run"))
)]
fn interrupt_chat_doc() {}

#[utoipa::path(
    get,
    path = "/api/models",
    tag = "models",
    responses((status = 200, description = "列出模型配置", body = [ModelViewDoc]))
)]
fn list_models_doc() {}

#[utoipa::path(
    post,
    path = "/api/models",
    tag = "models",
    request_body = ModelUpsertDoc,
    responses((status = 200, description = "创建模型", body = ModelViewDoc))
)]
fn create_model_doc() {}

#[utoipa::path(
    patch,
    path = "/api/models/{id}",
    tag = "models",
    params(("id" = String, Path, description = "模型配置 ID")),
    request_body = ModelUpsertDoc,
    responses((status = 200, description = "更新模型", body = ModelViewDoc))
)]
fn update_model_doc() {}

#[utoipa::path(
    delete,
    path = "/api/models/{id}",
    tag = "models",
    params(("id" = String, Path, description = "模型配置 ID")),
    responses((status = 204, description = "删除模型"))
)]
fn delete_model_doc() {}

#[utoipa::path(
    get,
    path = "/api/memory/search",
    tag = "memory",
    params(("q" = String, Query, description = "搜索关键词")),
    responses((status = 200, description = "Memory 检索结果"))
)]
fn memory_search_doc() {}

#[utoipa::path(
    get,
    path = "/api/mcp/servers",
    tag = "mcp",
    responses((status = 200, description = "MCP 服务列表", body = [McpServerDoc]))
)]
fn list_mcp_doc() {}

#[utoipa::path(
    post,
    path = "/api/mcp/reload",
    tag = "mcp",
    responses(
        (status = 200, description = "重载全局 MCP 连接池（不含 per-run filesystem 子进程；活跃 Run 时拒绝）"),
        (status = 409, description = "存在活跃 Run")
    )
)]
fn reload_mcp_doc() {}

/// Skill 摘要。
#[derive(utoipa::ToSchema, serde::Serialize)]
pub struct SkillSummaryDoc {
    /// Skill 名称。
    pub name: String,
    /// 描述。
    pub description: String,
    /// 源文件路径。
    pub file_path: String,
}

/// Skill 详情（含正文）。
#[derive(utoipa::ToSchema, serde::Serialize)]
pub struct SkillDetailDoc {
    /// Skill 名称。
    pub name: String,
    /// 描述。
    pub description: String,
    /// 源文件路径。
    pub file_path: String,
    /// Markdown 正文。
    pub content: String,
}

/// 附件元数据。
#[derive(utoipa::ToSchema, serde::Serialize)]
pub struct ArtifactDoc {
    /// 附件 ID。
    pub id: String,
    /// 文件名。
    pub filename: String,
    /// MIME 类型。
    pub mime_type: String,
    /// 大小（字节）。
    pub size_bytes: i64,
}

#[utoipa::path(
    get,
    path = "/api/skills",
    tag = "skills",
    responses((status = 200, description = "Skill 列表", body = [SkillSummaryDoc]))
)]
fn list_skills_doc() {}

#[utoipa::path(
    get,
    path = "/api/skills/{name}",
    tag = "skills",
    params(("name" = String, Path, description = "Skill 名称")),
    responses((status = 200, description = "Skill 详情", body = SkillDetailDoc))
)]
fn get_skill_doc() {}

#[utoipa::path(
    get,
    path = "/api/sessions/{id}/artifacts",
    tag = "sessions",
    params(("id" = String, Path, description = "会话 ID")),
    responses((status = 200, description = "附件列表", body = [ArtifactDoc]))
)]
fn list_artifacts_doc() {}

#[utoipa::path(
    post,
    path = "/api/sessions/{id}/artifacts",
    tag = "sessions",
    params(("id" = String, Path, description = "会话 ID")),
    responses((status = 200, description = "上传附件", body = ArtifactDoc))
)]
fn upload_artifact_doc() {}

#[utoipa::path(
    get,
    path = "/api/sessions/{id}/artifacts/{artifact_id}/preview",
    tag = "sessions",
    params(
        ("id" = String, Path, description = "会话 ID"),
        ("artifact_id" = String, Path, description = "附件 ID"),
    ),
    responses((status = 200, description = "附件预览", body = ArtifactPreviewDoc))
)]
fn preview_artifact_doc() {}

/// HITL 工具策略。
#[derive(utoipa::ToSchema, serde::Serialize)]
pub struct ToolPolicyDoc {
    /// 规则 ID。
    pub id: String,
    /// 工具名模式。
    pub tool_pattern: String,
    /// 来源类型。
    pub source_type: String,
    /// allow / confirm / deny。
    pub action: String,
    /// 是否启用。
    pub enabled: i64,
}

/// 工具策略创建/更新请求。
#[derive(utoipa::ToSchema, serde::Deserialize)]
pub struct ToolPolicyUpsertDoc {
    /// 工具名匹配模式。
    pub tool_pattern: String,
    /// 来源：`tool` / `mcp` / `builtin`。
    pub source_type: String,
    /// `allow` / `confirm` / `deny`。
    pub action: String,
    /// 是否启用。
    pub enabled: Option<bool>,
}

#[utoipa::path(
    get,
    path = "/api/tool-policies",
    tag = "governance",
    responses((status = 200, description = "工具策略列表", body = [ToolPolicyDoc]))
)]
fn list_tool_policies_doc() {}

#[utoipa::path(
    post,
    path = "/api/tool-policies",
    tag = "governance",
    request_body = ToolPolicyUpsertDoc,
    responses((status = 200, description = "创建工具策略", body = ToolPolicyDoc))
)]
fn create_tool_policy_doc() {}

#[utoipa::path(
    patch,
    path = "/api/tool-policies/{id}",
    tag = "governance",
    params(("id" = String, Path, description = "工具策略 ID")),
    request_body = ToolPolicyUpsertDoc,
    responses((status = 200, description = "更新工具策略", body = ToolPolicyDoc))
)]
fn update_tool_policy_doc() {}

#[utoipa::path(
    delete,
    path = "/api/tool-policies/{id}",
    tag = "governance",
    params(("id" = String, Path, description = "工具策略 ID")),
    responses((status = 204, description = "删除工具策略"))
)]
fn delete_tool_policy_doc() {}

#[utoipa::path(
    post,
    path = "/api/tool-policies/reload",
    tag = "governance",
    responses((status = 200, description = "从 DB 重载工具策略到 Harness"))
)]
fn reload_tool_policies_doc() {}

#[utoipa::path(
    get,
    path = "/api/tool-policies/worktree-guard",
    tag = "governance",
    responses((status = 200, description = "worktree 主仓库路径拦截开关", body = WorktreePathGuardDoc))
)]
fn get_worktree_path_guard_doc() {}

#[utoipa::path(
    patch,
    path = "/api/tool-policies/worktree-guard",
    tag = "governance",
    request_body = WorktreePathGuardDoc,
    responses((status = 200, description = "更新 worktree 路径守卫并热更新 Harness", body = WorktreePathGuardDoc))
)]
fn update_worktree_path_guard_doc() {}

#[utoipa::path(
    get,
    path = "/api/sessions/{id}/artifacts/{artifact_id}",
    tag = "sessions",
    params(
        ("id" = String, Path, description = "会话 ID"),
        ("artifact_id" = String, Path, description = "附件 ID"),
    ),
    responses((status = 200, description = "下载附件二进制"))
)]
fn download_artifact_doc() {}

#[utoipa::path(
    get,
    path = "/api/jobs",
    tag = "jobs",
    responses((status = 200, description = "后台任务列表", body = [JobDoc]))
)]
fn list_jobs_doc() {}

#[utoipa::path(
    post,
    path = "/api/auth/tokens",
    tag = "auth",
    request_body = CreateTokenDoc,
    responses((status = 200, description = "创建 API Token（明文仅返回一次）"))
)]
fn create_token_doc() {}

#[utoipa::path(
    get,
    path = "/api/auth/tokens",
    tag = "auth",
    responses((status = 200, description = "列出 Token（需 admin）"))
)]
fn list_tokens_doc() {}

/// OpenAPI 文档根（挂载于 `/api/docs`）。
#[derive(OpenApi)]
#[openapi(
    info(
        title = "maco API",
        version = "0.1.0",
        description = "个人 Agent 服务 — 会话、聊天（SSE）、模型、Memory、MCP、HITL/Elicitation、定时任务等。"
    ),
    paths(
        health_doc,
        guardrail_status_doc,
        pick_directory_doc,
        list_sessions_doc,
        create_session_doc,
        export_session_doc,
        session_messages_doc,
        get_plan_doc,
        put_plan_doc,
        list_todos_doc,
        patch_todo_doc,
        active_run_doc,
        provision_worktree_doc,
        get_run_doc,
        stream_run_doc,
        resume_run_doc,
        list_sub_agent_runs_doc,
        get_sub_agent_run_doc,
        cancel_sub_agent_doc,
        list_elicitation_doc,
        respond_elicitation_doc,
        chat_doc,
        interrupt_chat_doc,
        list_models_doc,
        create_model_doc,
        update_model_doc,
        delete_model_doc,
        memory_search_doc,
        list_mcp_doc,
        create_mcp_doc,
        update_mcp_doc,
        delete_mcp_doc,
        reload_mcp_doc,
        list_jobs_doc,
        create_token_doc,
        list_tokens_doc,
        list_skills_doc,
        get_skill_doc,
        list_artifacts_doc,
        upload_artifact_doc,
        preview_artifact_doc,
        download_artifact_doc,
        list_tool_policies_doc,
        create_tool_policy_doc,
        update_tool_policy_doc,
        delete_tool_policy_doc,
        reload_tool_policies_doc,
        get_worktree_path_guard_doc,
        update_worktree_path_guard_doc,
    ),
    components(schemas(
        HealthResponse,
        DependencyHealthDoc,
        McpServerStatusDoc,
        McpPoolHealthDoc,
        SessionMetaDoc,
        CreateSessionDoc,
        ChatRequestDoc,
        ModelViewDoc,
        ModelUpsertDoc,
        MemorySearchQueryDoc,
        GuardrailStatusDoc,
        WorktreePathGuardDoc,
        ActiveRunDoc,
        PickDirectoryDoc,
        RunStatusDoc,
        SseEnvelopeDoc,
        SseReplayMarkerPayloadDoc,
        PlanDoc,
        PutPlanDoc,
        TodoDoc,
        PatchTodoDoc,
        SubAgentRunDoc,
        ResumeRunDoc,
        ElicitationRespondDoc,
        McpServerDoc,
        JobDoc,
        CreateTokenDoc,
        ChatMessageDoc,
        SkillSummaryDoc,
        SkillDetailDoc,
        ArtifactDoc,
        ArtifactPreviewDoc,
        ToolPolicyDoc,
        ToolPolicyUpsertDoc,
    )),
    tags(
        (name = "system", description = "健康检查与治理"),
        (name = "sessions", description = "会话 CRUD 与导出"),
        (name = "runs", description = "Run 状态、HITL 恢复与 SSE 重连"),
        (name = "elicitation", description = "MCP Elicitation 人机交互"),
        (name = "chat", description = "聊天 SSE 与中断"),
        (name = "models", description = "LLM 模型配置"),
        (name = "memory", description = "长期记忆"),
        (name = "mcp", description = "MCP 服务配置"),
        (name = "jobs", description = "后台定时任务"),
        (name = "auth", description = "API Token 管理"),
        (name = "skills", description = "本地 Skill 扫描"),
        (name = "governance", description = "HITL 工具策略"),
    )
)]
pub struct ApiDoc;
