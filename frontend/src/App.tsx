import { useEffect, useRef, useState } from "react";
import {
  addMemory,
  createJob,
  createSession,
  deleteMemories,
  exportSessionUrl,
  fetchJobs,
  fetchMemories,
  fetchModels,
  fetchPlan,
  fetchTodos,
  fetchUsageSummary,
  getApiToken,
  type JobRecord,
  type ModelView,
  patchTodo,
  respondElicitation,
  resumeRun,
  runJobNow,
  streamChat,
} from "./api/client";
import { ModelSettings } from "./components/ModelSettings";
import { useChatStore, type Message } from "./store/chat";

type SseEvent = {
  type: string;
  run_id?: string;
  payload?: {
    content?: string;
    message?: string;
    tool_name?: string;
    elicitation_id?: string;
    request_type?: string;
    url?: string;
  };
};

type SidebarTab = "tasks" | "memory" | "usage" | "jobs" | "settings";

export default function App() {
  const { sessionId, messages, setSessionId, pushMessage, appendAssistant, reset } =
    useChatStore();
  const [input, setInput] = useState("");
  const [plan, setPlan] = useState("");
  const [todos, setTodos] = useState<Array<{ task_key: string; title: string; status: string }>>([]);
  const [loading, setLoading] = useState(false);
  const [pendingConfirm, setPendingConfirm] = useState<{ runId: string; toolName: string } | null>(null);
  const [pendingElicitation, setPendingElicitation] = useState<{
    id: string;
    requestType: string;
    message: string;
    url?: string;
  } | null>(null);
  const [elicitationInput, setElicitationInput] = useState("{}");
  const [usage, setUsage] = useState<Array<{ key: string; total_tokens: number; estimated_cost: number }>>([]);
  const [memories, setMemories] = useState<Array<{ id: number; content: string; timestamp: string }>>([]);
  const [memoryInput, setMemoryInput] = useState("");
  const [memoryDeleteQ, setMemoryDeleteQ] = useState("");
  const [models, setModels] = useState<ModelView[]>([]);
  const [selectedModelId, setSelectedModelId] = useState<string>("");
  const [sidebarTab, setSidebarTab] = useState<SidebarTab>("tasks");
  const [jobs, setJobs] = useState<JobRecord[]>([]);
  const [jobName, setJobName] = useState("");
  const chatEndRef = useRef<HTMLDivElement>(null);

  const defaultModel = models.find((m) => m.is_default) ?? models[0];

  useEffect(() => {
    fetchModels()
      .then((list) => {
        setModels(list);
        const def = list.find((m) => m.is_default) ?? list[0];
        if (def) setSelectedModelId(def.id);
      })
      .catch(() => setModels([]));
  }, []);

  useEffect(() => {
    fetchUsageSummary("model")
      .then((rows) =>
        setUsage(
          rows.map((r) => ({
            key: r.key,
            total_tokens: r.total_tokens,
            estimated_cost: r.estimated_cost,
          })),
        ),
      )
      .catch(() => setUsage([]));
    fetchMemories()
      .then((r) => setMemories(r.items))
      .catch(() => setMemories([]));
    fetchJobs()
      .then(setJobs)
      .catch(() => setJobs([]));
  }, [sessionId]);

  useEffect(() => {
    chatEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages, loading]);

  function handleSse(raw: Record<string, unknown>) {
    const ev = raw as SseEvent;
    if (ev.type === "text" && ev.payload?.content) {
      appendAssistant(ev.payload.content);
    }
    if (ev.type === "tool_confirm_request" && ev.run_id && ev.payload?.tool_name) {
      setPendingConfirm({ runId: ev.run_id, toolName: ev.payload.tool_name });
    }
    if (ev.type === "elicitation_request" && ev.payload?.elicitation_id) {
      setPendingElicitation({
        id: ev.payload.elicitation_id,
        requestType: ev.payload.request_type ?? "form",
        message: ev.payload.message ?? "Additional input required",
        url: ev.payload.url,
      });
    }
  }

  async function ensureSession() {
    if (sessionId) return sessionId;
    const modelId = selectedModelId || defaultModel?.id;
    const s = await createSession(undefined, modelId);
    setSessionId(s.session_id);
    return s.session_id;
  }

  async function refreshTasks(id: string) {
    const p = await fetchPlan(id);
    setPlan(p?.content ?? "");
    setTodos(await fetchTodos(id));
  }

  async function send() {
    if (!input.trim() || loading) return;
    if (!selectedModelId && models.length === 0) {
      pushMessage({
        role: "assistant",
        content: "请先在 Settings 中配置模型（API Key + Base URL + Model ID）",
      });
      setSidebarTab("settings");
      return;
    }
    setLoading(true);
    try {
      const sid = await ensureSession();
      pushMessage({ role: "user", content: input });
      const text = input;
      setInput("");
      await streamChat(sid, text, handleSse, selectedModelId || undefined);
      await refreshTasks(sid);
    } catch (e) {
      pushMessage({ role: "assistant", content: String(e) });
    } finally {
      setLoading(false);
    }
  }

  async function respondElicit(action: "accept" | "decline" | "cancel") {
    if (!pendingElicitation || loading) return;
    setLoading(true);
    try {
      let content: Record<string, unknown> | undefined;
      if (action === "accept") {
        content = JSON.parse(elicitationInput) as Record<string, unknown>;
      }
      await respondElicitation(pendingElicitation.id, action, content);
      setPendingElicitation(null);
      setElicitationInput("{}");
    } catch (e) {
      pushMessage({ role: "assistant", content: String(e) });
    } finally {
      setLoading(false);
    }
  }

  async function respondConfirm(approved: boolean) {
    if (!sessionId || !pendingConfirm || loading) return;
    setLoading(true);
    try {
      await resumeRun(sessionId, pendingConfirm.runId, approved, handleSse);
      setPendingConfirm(null);
      await refreshTasks(sessionId);
    } catch (e) {
      pushMessage({ role: "assistant", content: String(e) });
    } finally {
      setLoading(false);
    }
  }

  function exportMd() {
    if (!sessionId) return;
    const token = getApiToken();
    if (token) {
      fetch(exportSessionUrl(sessionId), {
        headers: { Authorization: `Bearer ${token}` },
      })
        .then((r) => r.blob())
        .then((blob) => {
          const url = URL.createObjectURL(blob);
          const a = document.createElement("a");
          a.href = url;
          a.download = `maco-session-${sessionId}.md`;
          a.click();
          URL.revokeObjectURL(url);
        })
        .catch((err) => pushMessage({ role: "assistant", content: String(err) }));
    } else {
      window.open(exportSessionUrl(sessionId), "_blank");
    }
  }

  const tabs: { id: SidebarTab; label: string }[] = [
    { id: "tasks", label: "Tasks" },
    { id: "memory", label: "Memory" },
    { id: "usage", label: "Usage" },
    { id: "jobs", label: "Jobs" },
    { id: "settings", label: "Settings" },
  ];

  return (
    <div className="app-shell">
      <div className="app-main">
        <header className="app-topbar">
          <div className="app-logo">ma<span>co</span></div>
          <select
            className="model-select"
            value={selectedModelId}
            onChange={(e) => setSelectedModelId(e.target.value)}
            title="Chat model"
          >
            {models.length === 0 ? (
              <option value="">No model — open Settings</option>
            ) : (
              models
                .filter((m) => m.enabled)
                .map((m) => (
                  <option key={m.id} value={m.id}>
                    {m.name} ({m.model_id})
                  </option>
                ))
            )}
          </select>
          <button
            type="button"
            className="btn btn-ghost btn-sm"
            onClick={() => {
              reset();
              setPlan("");
              setTodos([]);
            }}
          >
            New chat
          </button>
          {sessionId && (
            <button type="button" className="btn btn-sm" onClick={exportMd}>
              Export
            </button>
          )}
          <button
            type="button"
            className="btn btn-sm"
            onClick={() => setSidebarTab("settings")}
            style={{ marginLeft: "auto" }}
          >
            ⚙ Settings
          </button>
        </header>

        <div className="chat-scroll">
          {messages.length === 0 ? (
            <div className="chat-empty">
              <h2>Personal Agent</h2>
              <p>Configure a model in Settings, then start chatting.</p>
              {models.length === 0 && (
                <button
                  type="button"
                  className="btn btn-primary"
                  onClick={() => setSidebarTab("settings")}
                >
                  Add model
                </button>
              )}
            </div>
          ) : (
            messages.map((m: Message, i: number) => (
              <div key={i} className={`msg msg-${m.role}`}>
                <span className="msg-label">{m.role === "user" ? "You" : "Agent"}</span>
                <div className="msg-bubble">{m.content}</div>
              </div>
            ))
          )}
          {loading && (
            <div className="msg msg-assistant">
              <span className="msg-label">Agent</span>
              <div className="msg-bubble" style={{ opacity: 0.7 }}>
                Thinking…
              </div>
            </div>
          )}
          <div ref={chatEndRef} />
        </div>

        {pendingConfirm && (
          <div className="alert alert-warn">
            <strong>Tool confirmation</strong>
            <p style={{ margin: "6px 0 0" }}>{pendingConfirm.toolName}</p>
            <div className="alert-actions">
              <button type="button" className="btn btn-sm btn-primary" onClick={() => respondConfirm(true)} disabled={loading}>
                Approve
              </button>
              <button type="button" className="btn btn-sm" onClick={() => respondConfirm(false)} disabled={loading}>
                Reject
              </button>
            </div>
          </div>
        )}

        {pendingElicitation && (
          <div className="alert alert-info">
            <strong>MCP input required</strong>
            <p style={{ margin: "6px 0 0" }}>{pendingElicitation.message}</p>
            {pendingElicitation.url && (
              <a href={pendingElicitation.url} target="_blank" rel="noreferrer">
                Open URL
              </a>
            )}
            {pendingElicitation.requestType === "form" && (
              <textarea
                className="chat-input"
                style={{ width: "100%", marginTop: 8, minHeight: 72 }}
                value={elicitationInput}
                onChange={(e) => setElicitationInput(e.target.value)}
              />
            )}
            <div className="alert-actions">
              <button type="button" className="btn btn-sm btn-primary" onClick={() => respondElicit("accept")} disabled={loading}>
                Submit
              </button>
              <button type="button" className="btn btn-sm" onClick={() => respondElicit("decline")} disabled={loading}>
                Decline
              </button>
            </div>
          </div>
        )}

        <div className="chat-composer">
          <div className="chat-composer-inner">
            <textarea
              className="chat-input"
              rows={1}
              value={input}
              onChange={(e) => setInput(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && !e.shiftKey) {
                  e.preventDefault();
                  send();
                }
              }}
              placeholder="Message… (Enter to send, Shift+Enter for newline)"
              disabled={loading}
            />
            <button type="button" className="btn btn-primary" onClick={send} disabled={loading || !input.trim()}>
              Send
            </button>
          </div>
        </div>
      </div>

      <aside className="app-sidebar">
        <div className="sidebar-tabs">
          {tabs.map((t) => (
            <button
              key={t.id}
              type="button"
              className={`sidebar-tab ${sidebarTab === t.id ? "active" : ""}`}
              onClick={() => setSidebarTab(t.id)}
            >
              {t.label}
            </button>
          ))}
        </div>

        <div className="sidebar-panel">
          {sidebarTab === "tasks" && (
            <>
              <div className="panel-section">
                <h3>Plan</h3>
                <div className="panel-card">
                  <pre className="panel-pre">{plan || "No plan yet"}</pre>
                </div>
              </div>
              <div className="panel-section">
                <h3>Todos</h3>
                {todos.length === 0 ? (
                  <p style={{ fontSize: "0.85rem", color: "var(--text-muted)" }}>—</p>
                ) : (
                  todos.map((t) => (
                    <div key={t.task_key} className="todo-item">
                      <div>{t.title}</div>
                      <select
                        value={t.status}
                        onChange={async (e) => {
                          if (!sessionId) return;
                          await patchTodo(sessionId, t.task_key, e.target.value);
                          await refreshTasks(sessionId);
                        }}
                      >
                        <option value="pending">pending</option>
                        <option value="in_progress">in progress</option>
                        <option value="completed">completed</option>
                      </select>
                    </div>
                  ))
                )}
              </div>
            </>
          )}

          {sidebarTab === "memory" && (
            <div className="panel-section">
              <h3>Long-term memory</h3>
              {memories.map((m) => (
                <div key={m.id} className="memory-item">
                  <div className="memory-time">{m.timestamp}</div>
                  <div>{m.content}</div>
                </div>
              ))}
              <textarea
                className="chat-input"
                style={{ width: "100%", marginTop: 8, minHeight: 56 }}
                placeholder="Add memory…"
                value={memoryInput}
                onChange={(e) => setMemoryInput(e.target.value)}
              />
              <button
                type="button"
                className="btn btn-sm btn-primary"
                style={{ marginTop: 8 }}
                disabled={!memoryInput.trim()}
                onClick={async () => {
                  await addMemory(memoryInput.trim());
                  setMemoryInput("");
                  const r = await fetchMemories();
                  setMemories(r.items);
                }}
              >
                Save
              </button>
              <div style={{ display: "flex", gap: 6, marginTop: 10 }}>
                <input
                  style={{ flex: 1 }}
                  placeholder="Delete by keyword"
                  value={memoryDeleteQ}
                  onChange={(e) => setMemoryDeleteQ(e.target.value)}
                />
                <button
                  type="button"
                  className="btn btn-sm"
                  disabled={!memoryDeleteQ.trim()}
                  onClick={async () => {
                    await deleteMemories(memoryDeleteQ.trim());
                    setMemoryDeleteQ("");
                    const r = await fetchMemories();
                    setMemories(r.items);
                  }}
                >
                  Del
                </button>
              </div>
            </div>
          )}

          {sidebarTab === "usage" && (
            <div className="panel-section">
              <h3>Token usage</h3>
              {usage.length === 0 ? (
                <p style={{ fontSize: "0.85rem", color: "var(--text-muted)" }}>No usage yet</p>
              ) : (
                usage.map((u) => (
                  <div key={u.key} className="stat-row">
                    <span>{u.key}</span>
                    <span>
                      {u.total_tokens.toLocaleString()} tok
                      {u.estimated_cost > 0 ? ` · $${u.estimated_cost.toFixed(4)}` : ""}
                    </span>
                  </div>
                ))
              )}
            </div>
          )}

          {sidebarTab === "jobs" && (
            <div className="panel-section">
              <h3>Scheduled jobs</h3>
              {jobs.map((j) => (
                <div key={j.id} className="panel-card">
                  <strong>{j.name}</strong>
                  <div className="model-meta">{j.job_type} · {j.status}</div>
                  {j.next_run_at && <div className="model-meta">Next: {j.next_run_at}</div>}
                  <button
                    type="button"
                    className="btn btn-sm"
                    style={{ marginTop: 8 }}
                    onClick={async () => setJobs(await fetchJobs())}
                  >
                    Refresh
                  </button>
                  <button
                    type="button"
                    className="btn btn-sm btn-primary"
                    style={{ marginTop: 8, marginLeft: 6 }}
                    onClick={async () => {
                      await runJobNow(j.id);
                      setJobs(await fetchJobs());
                    }}
                  >
                    Run now
                  </button>
                </div>
              ))}
              <div className="field">
                <label>New ping job</label>
                <input
                  value={jobName}
                  onChange={(e) => setJobName(e.target.value)}
                  placeholder="Daily ping"
                />
              </div>
              <button
                type="button"
                className="btn btn-sm btn-primary"
                disabled={!jobName.trim()}
                onClick={async () => {
                  await createJob({
                    name: jobName.trim(),
                    job_type: "ping",
                    schedule: "hourly",
                    run_at: new Date().toISOString(),
                  });
                  setJobName("");
                  setJobs(await fetchJobs());
                }}
              >
                Create job
              </button>
            </div>
          )}

          {sidebarTab === "settings" && (
            <>
              <ModelSettings
                models={models}
                onChange={(list) => {
                  setModels(list);
                  const def = list.find((m) => m.is_default) ?? list[0];
                  if (def) setSelectedModelId(def.id);
                }}
              />
              <div className="panel-section" style={{ marginTop: 16 }}>
                <h3>API</h3>
                <a href="/api/docs" target="_blank" rel="noreferrer" className="btn btn-sm">
                  OpenAPI Docs
                </a>
              </div>
            </>
          )}
        </div>
      </aside>
    </div>
  );
}
