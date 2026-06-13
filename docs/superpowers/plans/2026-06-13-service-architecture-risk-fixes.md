# Service Architecture Risk Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 修复当前服务架构审计中第一轮高价值风险：MCP 健康状态、readonly 子 Agent 权限、子 Agent 取消状态、OpenAPI 漂移与基础验证链。

**Architecture:** 保持现有 crate 边界不做大拆分，优先在 `maco-harness` 暴露更可靠的运行态状态和工具选择规则，在 `maco-server` 补齐对外健康与 OpenAPI 契约。`routes.rs` 与 `AppState` 的结构性拆分需要独立重构计划承接，避免和本轮行为修复混在同一批变更里。

**Tech Stack:** Rust 2024、Axum、utoipa、Tokio、sqlx SQLite、ADK Runner/MCP。

---

## Task 1: MCP 健康状态可观测

**Files:**

- Modify: `crates/maco-harness/src/mcp_pool.rs`
- Modify: `crates/maco-server/src/routes.rs`
- Test: `crates/maco-harness/src/mcp_pool.rs`

- [ ] **Step 1: Write the failing test**

新增单元测试验证 `McpPoolStatus` 聚合时能表达 `degraded`，且 failed server 带有错误信息。

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p maco-harness mcp_pool`

- [ ] **Step 3: Write minimal implementation**

在 `McpPool` 内维护 `Vec<McpServerStatus>`，reload 时记录 stdio/SSE 启动结果，并新增 `health_status()`。`/api/health` 返回 `mcp_status` 与兼容的 `mcp` 摘要。

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p maco-harness mcp_pool`

## Task 2: readonly 子 Agent 权限收紧

**Files:**

- Modify: `crates/maco-harness/src/sub_agent.rs`
- Modify: `crates/maco-harness/src/harness.rs`
- Test: `crates/maco-harness/src/sub_agent.rs`

- [ ] **Step 1: Write the failing test**

新增单元测试验证 `readonly` profile 不允许 bash、filesystem MCP 或任意 MCP toolset，只允许明确只读的 artifact loader。

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p maco-harness sub_agent`

- [ ] **Step 3: Write minimal implementation**

将 worker 工具选择逻辑抽成可测试的 capability 规划函数，`Readonly` 不挂载 `bash_tool` 与 `mcp_toolsets`；`Coding/Full` 保持现有能力。

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p maco-harness sub_agent`

## Task 3: 子 Agent 取消状态落地更明确

**Files:**

- Modify: `crates/maco-harness/src/harness.rs`
- Modify: `crates/maco-harness/src/sub_agent.rs`
- Test: `crates/maco-harness/src/sub_agent.rs`

- [ ] **Step 1: Write the failing test**

新增单元测试验证取消事件 payload 包含 `task_key`、`reason` 和 `status=cancelled`。

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p maco-harness sub_agent`

- [ ] **Step 3: Write minimal implementation**

复用现有 `SubAgentRunRepo::finish(..., "cancelled", ...)`，并让 SSE cancel payload 明确带上 status；长调用仍由现有 timeout 兜底。

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p maco-harness sub_agent`

## Task 4: OpenAPI 补齐漂移端点

**Files:**

- Modify: `crates/maco-server/src/openapi.rs`

- [ ] **Step 1: Add docs for missing endpoints**

补齐 sub-agent runs、sub-agent cancel、artifact preview、todos、plan、tool policy write/delete、MCP delete 等已存在路由。

- [ ] **Step 2: Verify OpenAPI builds**

Run: `cargo check -p maco-server --no-default-features`

## Task 5: SSE 短断线回放

**Files:**

- Modify: `crates/maco-harness/src/run_stream.rs`
- Modify: `crates/maco-server/src/routes.rs`
- Modify: `crates/maco-server/src/openapi.rs`
- Test: `crates/maco-harness/src/run_stream.rs`

- [ ] **Step 1: Write the failing test**

新增单元测试验证 `RunStreamRegistry::subscribe_since(session_id, Some(last_seq))` 会回放内存中 `seq > last_seq` 的事件。

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p maco-harness run_stream`

- [ ] **Step 3: Write minimal implementation**

在活跃 Run hub 中维护最近 256 条 `SseEnvelope`，`publish()` 写入 replay buffer，`subscribe_since()` 在同一把锁内创建 broadcast receiver 并快照待回放事件。`GET /runs/{run_id}/stream?after_seq=N` 先输出 replay，再接实时广播。

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p maco-harness run_stream`

## Task 6: 基础 CI 验证链

**Files:**

- Create: `.github/workflows/ci.yml`

- [ ] **Step 1: Add CI workflow**

新增 GitHub Actions workflow，固定 Rust `1.94.0`，执行 `cargo fmt --all -- --check`、`cargo check -p maco-server --no-default-features`、`cargo test -p maco-harness mcp_pool`、`cargo test -p maco-harness run_stream`、`cargo test -p maco-harness sub_agent`。

- [ ] **Step 2: Verify commands locally**

Run: `cargo fmt && cargo test -p maco-harness mcp_pool && cargo test -p maco-harness run_stream && cargo test -p maco-harness sub_agent && cargo check -p maco-server --no-default-features`

## Task 7: 全量验证

**Files:**

- Verify only.

- [ ] **Step 1: Format**

Run: `cargo fmt`

- [ ] **Step 2: Targeted tests**

Run: `cargo test -p maco-harness mcp_pool`

Run: `cargo test -p maco-harness run_stream`

Run: `cargo test -p maco-harness sub_agent`

- [ ] **Step 3: Server check**

Run: `cargo check -p maco-server --no-default-features`

## Follow-Up: `routes.rs` / `AppState` 结构拆分

本轮已经把最危险的运行态行为下沉到 `maco-harness` 的专门组件中：MCP 健康状态在 `McpPool` 内聚合，SSE replay 在 `RunStreamRegistry` 内维护，子 Agent 工具 profile 在 `sub_agent.rs` 中有可测试规划函数。  

`routes.rs` 仍然承担大量 handler、DTO 与错误映射，`AppState` 仍是集中式依赖容器。建议后续单独做一轮机械拆分：

- `routes/run_routes.rs`：Run 状态、stream、resume、sub-agent 查询/取消。
- `routes/mcp_routes.rs`：MCP server CRUD、reload、health DTO。
- `routes/artifact_routes.rs`：artifact list/upload/download/preview。
- `routes/governance_routes.rs`：tool policy 与 guardrail status。

这类拆分应先只移动代码不改行为，配套 `cargo check -p maco-server --no-default-features`，避免和功能修复同批引入路由回归。
