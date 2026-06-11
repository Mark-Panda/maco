# maco

个人 Agent 代理服务。

基于 Rust Cargo workspace（[adk-rust](https://github.com/zavora-ai/adk-rust) + [axum](https://github.com/tokio-rs/axum) + sqlx/SQLite）+ React 构建。**Session / Memory 与 adk 原生 SQLite 对齐**；maco 业务数据落 `~/.maco/data/maco.db`。

## 技术栈

| 层级 | 选型 |
| --- | --- |
| 后端语言 | Rust 2024 |
| Agent 框架 | adk-rust `=1.0.0`（`features = ["standard"]`，pin 版本） |
| HTTP 框架 | axum |
| 数据库 | SQLite 三文件（见下） |
| ORM | sqlx（仅 maco.db） |
| 前端 | React 18 + TypeScript + Vite + shadcn/ui + Tailwind + Zustand |

## 功能概览

### 后端

- **Harness**：`Runner` + `LlmAgentBuilder` + `RunOrchestrator` + `McpPool`
- **SessionFacade**：统一 adk session 与 maco meta 一致性（含补偿 reconcile）
- **SKILL / MCP** 可插拔（Phase 1 Skill 仅文件扫描；MCP 走 DB）
- **ReAct**：plan/todo 经显式 Agent Tool + 用户 PATCH 完成态
- **6 类 Callback 日志** + SSE 统一 Envelope
- **Session / Memory**：adk SQLite（路径以 Phase 0 spike 为准）

### 前端

- 流式对话、多模态上传、任务看板
- Phase 2：HITL、Elicitation、Memory UI、用量统计等

---

## 存储架构（SQLite 三文件）

| 文件 | 内容 | 管理方 |
| --- | --- | --- |
| `maco.db` | models、react、runs、logs、plugins、settings | sqlx |
| `sessions.db` | 对话 events、session state | **adk Session** |
| `memory.db` | 长期记忆 | **adk Memory** |
| `artifacts/` | 多模态二进制 | 本地文件 |

adk 固定：`app_name = "maco"`，`user_id = "local"`。

```toml
# ~/.maco/config.toml
[data]
maco_db       = "~/.maco/data/maco.db"
sessions_db   = "~/.maco/data/sessions.db"
memory_db     = "~/.maco/data/memory.db"
artifacts_dir = "~/.maco/data/artifacts"
```

---

## 风险治理（R1–R27）

| 风险 | 解决方案 | 阶段 |
| --- | --- | --- |
| R1 双库 Session | **SessionFacade** + reconcile | P1 |
| R2 Run 续跑 | **spike**；P1 三态；P2 **重开 run** | P0/P2 |
| R3 Memory embedding | **`maco_app_settings.memory`** | P1 |
| R4 McpPool | 状态机 + acquire/release | P1 |
| R5 三库运维 | WAL + `maco backup` + 日志告警 | P1 |
| R6 Session 列表 | adk 为主 + 批量补 meta | P1 |
| R7 ReAct 写入 | Tool 写 plan/todo；完成态仅 PATCH | P1 |
| R8 SSE 断线 | 断线不 cancel；重连拉 run + history | P1 |
| R9 范围过大 | **Phase 1a/1b 拆分** | P1 |
| R10 无鉴权 | 默认 **127.0.0.1**；P2 Bearer | P1/P2 |
| R11 adk API 路径 | **spike 确认 crate/feature** | P0 |
| R12 PG 残留 DDL | **全 SQLite 化** | P0 |
| R13 spike 未做 | **阻塞 Phase1** | P0 |
| R14 跨库补偿 | **pending_delete / orphan + reconcile 队列** | P1 |
| R15 model 双源 | **adk state 为运行时真相** | P1 |
| R16 并发 run | **session Mutex + 唯一索引** | P1 |
| R17 resume_context | **固定 JSON schema + spike 验证** | P0/P2 |
| R18 Memory 检索 | **API 返回 search_mode** | P1 |
| R19 Callback 膨胀 | **truncate + 基础 redact + retention** | P1 |
| R20 Artifact 安全 | **20MB / MIME / 路径约束** | P1 |
| R21 SSE 重连 | **GET run 含 last_seq + pending_tools** | P1 |
| R22 plan 并发 | **version 乐观锁 409** | P1 |
| R23 Skill 双源 | **P1 仅 ~/.maco/skills/** | P1 |
| R24 非原子备份 | **WAL checkpoint + best-effort** | P1 |
| R25 adk 升级 | **pin 版本 + UPGRADING.md** | P1 |
| R26 Phase1 偏大 | **1a Chat / 1b MCP+ReAct** | P1 |
| R27 本机安全 | **本机信任 + P1 基础 redact** | P1/P2 |

### Phase 0（必须先做）

`crates/maco-harness/examples/run_spike.rs` → 产出 `docs/spike-report.md`：

1. Session/Memory SQLite 真实 import 路径与 Cargo features（R11）
2. `Runner::interrupt` 语义
3. 新 run 注入 tool result 可行性（R17）
4. Memory search 能力（R18）

### 关键契约

**SessionFacade（R1/R14）**：`status` 含 `pending_delete | orphan_create`；失败入 reconcile 重试。

**RunOrchestrator（R2/R16）**：P1 三态 `pending → running → completed | failed | cancelled`；同 session 串行 run。

**model（R15）**：运行时以 adk `user:model` 为准；`SessionFacade::set_model` 双写 meta + state。

**resume_context（R17）**：

```json
{
  "schema_version": 1,
  "reason": "hitl | elicitation",
  "parent_run_id": "...",
  "pending_tool_call": { "name": "...", "args": {}, "call_id": "..." },
  "do_not_replay_events": true
}
```

**ReAct（R7/R22）**：Agent Tool 写 plan/todo；`PUT /plan` 带 `version`；todo 完成仅用户 PATCH。

**SSE（R8/R21）**：断线不 cancel；`GET /runs/:id` 返回 `{ status, last_seq, pending_tools[] }`。

---

## 项目结构

```
maco/
├── crates/
│   ├── maco-core/
│   ├── maco-db/
│   ├── maco-storage/
│   ├── maco-harness/
│   ├── maco-react/
│   ├── maco-telemetry/
│   ├── maco-governance/   # Phase 2
│   └── maco-server/
├── migrations/
├── docs/spike-report.md   # Phase 0 产出
├── scripts/init.sh
└── frontend/
```

---

## maco.db 核心表

```sql
CREATE TABLE maco_session_meta (
    session_id  TEXT PRIMARY KEY,
    title       TEXT,
    model_id    TEXT REFERENCES maco_models(id),
    project_id  TEXT,
    status      TEXT NOT NULL DEFAULT 'active'
                CHECK (status IN ('active','archived','pending_delete','deleted','orphan_create')),
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);

CREATE TABLE maco_runs (
    id              TEXT PRIMARY KEY,
    session_id      TEXT NOT NULL,
    status          TEXT NOT NULL,
    resume_context  TEXT,
    superseded_by   TEXT,
    error_message   TEXT,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);
CREATE UNIQUE INDEX idx_maco_runs_session_active
    ON maco_runs(session_id) WHERE status = 'running';

CREATE TABLE maco_react_plans (
    session_id  TEXT PRIMARY KEY,
    content     TEXT NOT NULL DEFAULT '',
    version     INTEGER NOT NULL DEFAULT 1,
    updated_at  TEXT NOT NULL
);
```

Phase 2：`maco_api_tokens`、`maco_tool_policies`、`maco_usage_stats`、`maco_elicitation_requests`、`maco_jobs`（全 SQLite，无 `maco_users`）

---

## HTTP API（Phase 1）

```
GET/POST/PATCH/DELETE  /api/sessions
GET/POST/PATCH/DELETE  /api/models
GET/PUT                /api/sessions/:id/plan          # PUT 带 version
GET/PATCH              /api/sessions/:id/todos/:task_key
POST                   /api/chat                       # SSE Envelope
POST                   /api/chat/:id/interrupt
GET                    /api/sessions/:id/runs/:run_id  # status, last_seq, pending_tools
GET                    /api/memory/search              # 含 search_mode
GET                    /health
```

---

## Phase 划分

### Phase 0

adk spike → `docs/spike-report.md`

### Phase 1a（Chat 核心）

Workspace、migrations、SessionFacade、三态 Run、Chat SSE、模型、127.0.0.1

### Phase 1b（扩展）

McpPool、ReAct Tools、Callback 日志、Skill 文件扫描、前端 MVP

### Phase 2

鉴权、HITL（重开 run）、用量、Elicitation、Memory UI、Jobs、导出、Guardrail、OpenAPI

---

## 部署

```bash
cp .env.example .env
cargo run -p maco-server -- init
cargo run -p maco-server -- --bind 127.0.0.1:8080
cargo run -p maco-server -- backup   # checkpoint 三库 + artifacts（best-effort）
```

---

## 参考链接

- [adk-rust](https://github.com/zavora-ai/adk-rust)
- [adk-rust 文档](https://docs.rs/adk-rust/latest/adk_rust/)
- [adk-session](https://docs.rs/adk-session/latest/adk_session/)（SQLite backend 可能在独立 crate）
