# ReAct Sub-Agent Implementation Plan

> **Goal:** 主 Agent 通过 `spawn_sub_agent` 按需启动 adk `LlmAgent` 子 worker 执行 ReAct todo。

**Architecture:** `SubAgentRunContext` + `SpawnSubAgentTool` 嵌套 `InvocationContext::run`；事件写入 session，`tool_call` / `sub_agent_progress` SSE 转发。

**Tech Stack:** adk-agent `LlmAgentBuilder`, adk-runner `InvocationContext`, maco-harness, React SSE。

---

## v1 — 已完成

- [x] `crates/maco-harness/src/sub_agent.rs` — executor、工具、限制、测试
- [x] `harness.rs` — 注册工具、instruction
- [x] `callbacks.rs` — 共享 `agent_guardrails`
- [x] `frontend/App.tsx` — 子 Agent tool_call / progress activity

## Phase 2a — 已完成

- [x] E1 `sub_agent_progress` SSE
- [x] E2 任务看板 Sub-Agent 泳道 UI
- [x] E5 `maco_sub_agent_runs` 审计表 + API
- [x] E7 子 Agent 取消 API

## Phase 2b — 进行中

- [x] E3 `MutableSession` 共享与 `sub/{task_key}` 分支
- [x] E4 `SharedState` 结构化产物（`subagent:{task_key}:*`）
- [ ] E6 子 Agent 独立模型
- [ ] E9 `tools_profile` MCP 白名单

## Phase 2c–4

见 `docs/superpowers/specs/2026-06-12-react-sub-agent-design.md` 扩展章节。
