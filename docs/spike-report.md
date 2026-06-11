# Phase 0 Spike Report (R11/R13/R17/R18)

Date: 2026-06-11  
Command: `cargo run -p maco-harness --example run_spike`

## R11 — adk SQLite API paths

| Component | Crate | Type | Feature | URL format |
|-----------|-------|------|---------|------------|
| Session | `adk_session` | `SqliteSessionService` | `sqlite` | `sqlite:///absolute/path?mode=rwc` (file) or `sqlite::memory:` |
| Memory | `adk_memory` | `SqliteMemoryService` | `sqlite-memory` | `sqlite:///absolute/path` (`create_if_missing`) |
| maco sqlx | `sqlx` | `SqlitePool` | — | `sqlite://path?mode=rwc` (two slashes) |

**File-based pitfall (R11):** adk session requires **three slashes** (`sqlite:///…`) plus `?mode=rwc` for create-if-missing. sqlx uses two slashes. Helpers live in `maco-core::config`: `maco_db_url`, `adk_session_url`, `adk_memory_url`.

**Not** re-exported from `adk_rust::prelude` (only `InMemory*` services). maco uses direct crate deps.

Workspace features:

```toml
adk-rust = { version = "=1.0.0", features = ["standard", "sqlite-memory"] }
adk-session = { version = "=1.0.0", features = ["sqlite"] }
adk-memory = { version = "=1.0.0", features = ["sqlite-memory"] }
```

Both services require `migrate().await?` after `new()`.

## R13 — Runner interrupt

- API: `Runner::interrupt(session_id: &str) -> bool`
- Per-session cancellation token registered during `run()`
- Events already appended are preserved
- New `run()` with updated `Content` is the supported redirect path

## R17 — Resume / HITL default

Default maco strategy: **new run_id + inject tool result** via `Content` with `FunctionResponse` part.  
`resume_context` schema defined in `maco-core` types.

## R18 — Memory search

`SqliteMemoryService` uses **FTS5 keyword search only** — no embedding/semantic search.  
maco API returns `search_mode: "keyword"` explicitly.

## Toolchain

- `adk-rust 1.0.0` requires **Rust 1.94.0** (see `rust-toolchain.toml`)

## Decision

Proceed with Phase 1 using `adk_session` / `adk_memory` directly; Phase 2 HITL via re-open run.
