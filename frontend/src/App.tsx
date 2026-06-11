import { useEffect, useRef, useState } from "react";
import {
  addMemory,
  createApiToken,
  createJob,
  createSession,
  deleteMemories,
  exportSessionUrl,
  deleteSession,
  fetchActiveRun,
  fetchJobs,
  fetchMemories,
  fetchModels,
  fetchPlan,
  fetchRun,
  fetchSessionMessages,
  fetchTodos,
  fetchUsageSummary,
  getApiToken,
  getLastModelId,
  getLastSessionId,
  getSidebarVisible,
  getSidebarWidth,
  persistSidebarVisible,
  persistSidebarWidth,
  SIDEBAR_WIDTH_DEFAULT,
  SIDEBAR_WIDTH_MAX,
  SIDEBAR_WIDTH_MIN,
  interruptChat,
  listApiTokens,
  listArtifacts,
  listSessions,
  setLastModelId,
  setLastSessionId,
  updateSession,
  type JobRecord,
  type ModelView,
  type SessionMeta,
  patchTodo,
  pickProjectDirectory,
  respondElicitation,
  resumeRun,
  revokeApiToken,
  runJobNow,
  searchMemory,
  setApiToken,
  streamChat,
  streamRun,
  uploadArtifact,
} from "./api/client";
import { MacoIcon, type MacoIconName } from "./components/Icons";
import { McpSettings } from "./components/McpSettings";
import { ModelSettings } from "./components/ModelSettings";
import { ArtifactPreviewModal } from "./components/ArtifactPreviewModal";
import { SkillsPanel } from "./components/SkillsPanel";
import { ToolPolicySettings } from "./components/ToolPolicySettings";
import { useChatStore, type Message } from "./store/chat";
import { applyTheme, getTheme, type Theme, toggleTheme as flipTheme } from "./theme";

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
    id?: string;
    filename?: string;
    mime_type?: string;
    size_bytes?: number;
  };
};

type SidebarTab = "sessions" | "tasks" | "memory" | "skills" | "usage" | "jobs" | "settings";

export default function App() {
  const {
    sessionId,
    messages,
    setSessionId,
    setMessages,
    pushMessage,
    appendAssistant,
    reset,
    clearMessages,
  } = useChatStore();
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
  const [theme, setTheme] = useState<Theme>(getTheme);
  const [sidebarOpen, setSidebarOpen] = useState(getSidebarVisible);
  const [sidebarWidth, setSidebarWidth] = useState(getSidebarWidth);
  const sidebarWidthRef = useRef(sidebarWidth);

  useEffect(() => {
    sidebarWidthRef.current = sidebarWidth;
  }, [sidebarWidth]);

  useEffect(() => {
    applyTheme(theme);
  }, [theme]);

  function onToggleTheme() {
    setTheme((current) => flipTheme(current));
  }

  function clampSidebarWidth(width: number) {
    const maxByViewport = Math.max(SIDEBAR_WIDTH_MIN, window.innerWidth - 320);
    return Math.round(
      Math.min(SIDEBAR_WIDTH_MAX, maxByViewport, Math.max(SIDEBAR_WIDTH_MIN, width)),
    );
  }

  function onSidebarResizeStart(e: React.PointerEvent<HTMLDivElement>) {
    e.preventDefault();
    const startX = e.clientX;
    const startWidth = sidebarWidthRef.current;
    const handle = e.currentTarget;
    handle.setPointerCapture(e.pointerId);
    document.body.classList.add("sidebar-resizing");

    const onMove = (ev: PointerEvent) => {
      const delta = startX - ev.clientX;
      setSidebarWidth(clampSidebarWidth(startWidth + delta));
    };

    const onEnd = (ev: PointerEvent) => {
      if (handle.hasPointerCapture(ev.pointerId)) {
        handle.releasePointerCapture(ev.pointerId);
      }
      document.body.classList.remove("sidebar-resizing");
      persistSidebarWidth(sidebarWidthRef.current);
      handle.removeEventListener("pointermove", onMove);
      handle.removeEventListener("pointerup", onEnd);
      handle.removeEventListener("pointercancel", onEnd);
    };

    handle.addEventListener("pointermove", onMove);
    handle.addEventListener("pointerup", onEnd);
    handle.addEventListener("pointercancel", onEnd);
  }

  function resetSidebarWidth() {
    setSidebarWidth(SIDEBAR_WIDTH_DEFAULT);
    persistSidebarWidth(SIDEBAR_WIDTH_DEFAULT);
  }

  function openSidebar(tab?: SidebarTab) {
    setSidebarOpen(true);
    persistSidebarVisible(true);
    if (tab) setSidebarTab(tab);
  }

  function closeSidebar() {
    setSidebarOpen(false);
    persistSidebarVisible(false);
  }

  function toggleSidebar() {
    if (sidebarOpen) closeSidebar();
    else openSidebar();
  }
  const [jobs, setJobs] = useState<JobRecord[]>([]);
  const [jobName, setJobName] = useState("");
  const [sessions, setSessions] = useState<SessionMeta[]>([]);
  const [activeRunId, setActiveRunId] = useState<string | null>(null);
  const [memorySearchQ, setMemorySearchQ] = useState("");
  const [memorySearchHits, setMemorySearchHits] = useState<
    Array<{ id: number; content: string; timestamp: string }>
  >([]);
  const [tokenName, setTokenName] = useState("");
  const [tokenList, setTokenList] = useState<
    Array<{ id: string; name: string; created_at: string }>
  >([]);
  const [newToken, setNewToken] = useState<string | null>(null);
  const [artifacts, setArtifacts] = useState<
    Array<{ id: string; filename: string; mime_type: string; size_bytes: number }>
  >([]);
  const [previewArtifactItem, setPreviewArtifactItem] = useState<{
    id: string;
    filename: string;
    mime_type: string;
    size_bytes: number;
  } | null>(null);
  const [editingTitle, setEditingTitle] = useState(false);
  const [titleDraft, setTitleDraft] = useState("");
  const [projectRootDraft, setProjectRootDraft] = useState("");
  const [pickingFolder, setPickingFolder] = useState(false);
  const [restored, setRestored] = useState(false);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const chatAbortRef = useRef<AbortController | null>(null);
  const chatEndRef = useRef<HTMLDivElement>(null);

  const defaultModel = models.find((m) => m.is_default) ?? models[0];

  useEffect(() => {
    fetchModels()
      .then((list) => {
        setModels(list);
        const savedModel = getLastModelId();
        const def = list.find((m) => m.id === savedModel)
          ?? list.find((m) => m.is_default)
          ?? list[0];
        if (def) setSelectedModelId(def.id);
      })
      .catch(() => setModels([]));
  }, []);

  useEffect(() => {
    if (restored || models.length === 0) return;
    const saved = getLastSessionId();
    if (!saved) {
      setRestored(true);
      return;
    }
    listSessions()
      .then((rows) => {
        setSessions(rows);
        const s = rows.find((r) => r.session_id === saved);
        return loadSession(saved, s?.model_id, s?.project_root);
      })
      .finally(() => setRestored(true));
  }, [models, restored]);

  useEffect(() => {
    setLastSessionId(sessionId);
  }, [sessionId]);

  useEffect(() => {
    if (selectedModelId) setLastModelId(selectedModelId);
  }, [selectedModelId]);

  useEffect(() => {
    listSessions()
      .then(setSessions)
      .catch(() => setSessions([]));
    listApiTokens()
      .then((rows) =>
        setTokenList(rows.map((r) => ({ id: r.id, name: r.name, created_at: r.created_at }))),
      )
      .catch(() => setTokenList([]));
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
    const ev = raw as SseEvent & { event_type?: string };
    const type = ev.type ?? ev.event_type;
    if (ev.run_id) setActiveRunId(ev.run_id);
    if (type === "text" && ev.payload?.content) {
      appendAssistant(ev.payload.content);
    }
    if (type === "tool_confirm_request" && ev.run_id && ev.payload?.tool_name) {
      setPendingConfirm({ runId: ev.run_id, toolName: ev.payload.tool_name });
    }
    if (type === "elicitation_request" && ev.payload?.elicitation_id) {
      setPendingElicitation({
        id: ev.payload.elicitation_id,
        requestType: ev.payload.request_type ?? "form",
        message: ev.payload.message ?? "需要补充输入",
        url: ev.payload.url,
      });
    }
    if (type === "artifact_created" && ev.payload?.id && ev.payload.filename) {
      const item = {
        id: ev.payload.id,
        filename: ev.payload.filename,
        mime_type: ev.payload.mime_type ?? "application/octet-stream",
        size_bytes: ev.payload.size_bytes ?? 0,
      };
      setArtifacts((prev) => {
        if (prev.some((a) => a.id === item.id)) return prev;
        return [item, ...prev];
      });
    }
    if (type === "done" && sessionId) {
      listArtifacts(sessionId).then(setArtifacts).catch(() => undefined);
    }
  }

  async function ensureSession() {
    if (sessionId) return sessionId;
    const modelId = selectedModelId || defaultModel?.id;
    const root = projectRootDraft.trim() || undefined;
    const s = await createSession(undefined, modelId, root);
    setSessionId(s.session_id);
    setSessions(await listSessions());
    return s.session_id;
  }

  async function refreshTasks(id: string) {
    const p = await fetchPlan(id);
    setPlan(p?.content ?? "");
    setTodos(await fetchTodos(id));
  }

  async function loadSession(
    sid: string,
    modelId?: string | null,
    projectRoot?: string | null,
  ) {
    setSessionId(sid);
    if (modelId) setSelectedModelId(modelId);
    setProjectRootDraft(projectRoot ?? "");
    setActiveRunId(null);
    setPendingConfirm(null);
    setPendingElicitation(null);
    try {
      const hist = await fetchSessionMessages(sid);
      setMessages(
        hist.messages.map((m) => ({
          role: m.role === "user" ? "user" : "assistant",
          content: m.content,
        })),
      );
      await refreshTasks(sid);
      const active = await fetchActiveRun(sid);
      if (active.run_id) {
        setActiveRunId(active.run_id);
        try {
          const run = await fetchRun(sid, active.run_id);
          if (run.pending_tools.length > 0) {
            setPendingConfirm({
              runId: active.run_id,
              toolName: run.pending_tools[0].name,
            });
          }
          if (run.pending_elicitations.length > 0) {
            const e = run.pending_elicitations[0];
            setPendingElicitation({
              id: e.id,
              requestType: e.request_type || "form",
              message: e.message || "需要补充输入",
              url: e.url,
            });
          }
          if (run.status === "running" || run.status === "awaiting_user") {
            setLoading(true);
            await streamRun(sid, active.run_id, handleSse);
          }
        } catch {
          // 活跃 Run 可能已结束，忽略重连失败
        } finally {
          setLoading(false);
        }
      }
      const arts = await listArtifacts(sid);
      setArtifacts(arts);
    } catch {
      clearMessages();
      setArtifacts([]);
    }
  }

  const currentSession = sessions.find((s) => s.session_id === sessionId);

  async function saveSessionTitle() {
    if (!sessionId || !titleDraft.trim()) {
      setEditingTitle(false);
      return;
    }
    await updateSession(sessionId, { title: titleDraft.trim() });
    setSessions(await listSessions());
    setEditingTitle(false);
  }

  async function pickProjectFolder() {
    setPickingFolder(true);
    try {
      const result = await pickProjectDirectory();
      if (result.cancelled || !result.path) return;
      setProjectRootDraft(result.path);
      if (sessionId) {
        await updateSession(sessionId, { project_root: result.path });
        setSessions(await listSessions());
      }
    } catch (e) {
      pushMessage({ role: "assistant", content: String(e) });
    } finally {
      setPickingFolder(false);
    }
  }

  async function clearProjectRoot() {
    setProjectRootDraft("");
    if (!sessionId) return;
    try {
      await updateSession(sessionId, { project_root: "" });
      setSessions(await listSessions());
    } catch (e) {
      pushMessage({ role: "assistant", content: String(e) });
    }
  }

  async function send() {
    if (!input.trim() || loading) return;
    if (!selectedModelId && models.length === 0) {
      pushMessage({
        role: "assistant",
        content: "请先在「设置」中配置模型（API Key + Base URL + Model ID）",
      });
      openSidebar("settings");
      return;
    }
    setLoading(true);
    try {
      const sid = await ensureSession();
      const isFirstMessage = messages.length === 0;
      pushMessage({ role: "user", content: input });
      const text = input;
      setInput("");
      if (isFirstMessage) {
        const autoTitle = text.trim().slice(0, 48) + (text.length > 48 ? "…" : "");
        updateSession(sid, { title: autoTitle })
          .then(() => listSessions().then(setSessions))
          .catch(() => undefined);
      }
      chatAbortRef.current?.abort();
      const ac = new AbortController();
      chatAbortRef.current = ac;
      await streamChat(sid, text, handleSse, selectedModelId || undefined, ac.signal);
      await refreshTasks(sid);
      setArtifacts(await listArtifacts(sid));
      listSessions().then(setSessions).catch(() => undefined);
    } catch (e) {
      pushMessage({ role: "assistant", content: String(e) });
    } finally {
      setLoading(false);
    }
  }

  async function reconnectActiveStream(sid: string) {
    const active = await fetchActiveRun(sid);
    if (!active.run_id) return;
    setActiveRunId(active.run_id);
    await streamRun(sid, active.run_id, handleSse);
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
      if (sessionId) {
        await reconnectActiveStream(sessionId);
        await refreshTasks(sessionId);
      }
    } catch (e) {
      pushMessage({ role: "assistant", content: String(e) });
    } finally {
      setLoading(false);
    }
  }

  async function stopRun() {
    if (!sessionId || !loading) return;
    chatAbortRef.current?.abort();
    try {
      await interruptChat(sessionId);
      setActiveRunId(null);
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

  const tabs: { id: SidebarTab; label: string; icon: MacoIconName; desc: string }[] = [
    { id: "sessions", label: "会话", icon: "sessions", desc: "历史对话与切换" },
    { id: "tasks", label: "任务", icon: "tasks", desc: "计划、待办与附件" },
    { id: "memory", label: "记忆", icon: "memory", desc: "长期记忆读写" },
    { id: "skills", label: "技能", icon: "skills", desc: "本地 Skill 文件" },
    { id: "usage", label: "用量", icon: "usage", desc: "Token 与费用统计" },
    { id: "jobs", label: "调度", icon: "jobs", desc: "定时后台任务" },
    { id: "settings", label: "设置", icon: "settings", desc: "模型、MCP 与鉴权" },
  ];

  const activeTab = tabs.find((t) => t.id === sidebarTab) ?? tabs[0];

  const shellStyle = sidebarOpen
    ? ({ "--sidebar-width": `${sidebarWidth}px` } as React.CSSProperties)
    : undefined;

  return (
    <div className={`app-shell${sidebarOpen ? "" : " sidebar-closed"}`} style={shellStyle}>
      <div className="app-main">
        <header className="app-topbar">
          <div className="app-logo">ma<span>co</span></div>
          {sessionId && (
            editingTitle ? (
              <input
                className="model-select"
                style={{ maxWidth: 200 }}
                value={titleDraft}
                onChange={(e) => setTitleDraft(e.target.value)}
                onBlur={saveSessionTitle}
                onKeyDown={(e) => {
                  if (e.key === "Enter") saveSessionTitle();
                  if (e.key === "Escape") setEditingTitle(false);
                }}
                autoFocus
              />
            ) : (
              <button
                type="button"
                className="btn btn-ghost btn-sm"
                title="点击重命名"
                onClick={() => {
                  setTitleDraft(currentSession?.title ?? sessionId.slice(0, 8));
                  setEditingTitle(true);
                }}
              >
                {currentSession?.title ?? sessionId.slice(0, 8)}
              </button>
            )
          )}
          {sessionId && currentSession?.project_root && (
            <span className="topbar-project" title={currentSession.project_root}>
              项目: {currentSession.project_root}
            </span>
          )}
          <select
            className="model-select"
            value={selectedModelId}
            onChange={(e) => setSelectedModelId(e.target.value)}
            title="对话模型"
          >
            {models.length === 0 ? (
              <option value="">无模型 — 请打开设置</option>
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
              setLastSessionId(null);
              setArtifacts([]);
              setPlan("");
              setTodos([]);
              setProjectRootDraft("");
            }}
          >
            新对话
          </button>
          {sessionId && (
            <button type="button" className="btn btn-sm" onClick={exportMd}>
              导出
            </button>
          )}
          {loading && sessionId && (
            <button type="button" className="btn btn-sm" onClick={stopRun}>
              停止
            </button>
          )}
          {activeRunId && !loading && sessionId && (
            <button
              type="button"
              className="btn btn-sm btn-ghost"
              title="重新连接 SSE"
              onClick={async () => {
                setLoading(true);
                try {
                  await streamRun(sessionId, activeRunId, handleSse);
                } catch (e) {
                  pushMessage({ role: "assistant", content: String(e) });
                } finally {
                  setLoading(false);
                }
              }}
            >
              重连
            </button>
          )}
          <div className="topbar-actions">
            <button
              type="button"
              className="btn btn-sm btn-ghost btn-icon-only theme-toggle"
              onClick={onToggleTheme}
              title={theme === "dark" ? "切换浅色模式" : "切换深色模式"}
              aria-label={theme === "dark" ? "切换浅色模式" : "切换深色模式"}
            >
              <MacoIcon name={theme === "dark" ? "sun" : "moon"} size={18} />
            </button>
            <button
              type="button"
              className="btn btn-sm btn-ghost sidebar-toggle"
              onClick={toggleSidebar}
              aria-expanded={sidebarOpen}
              title={sidebarOpen ? "收起右侧工具栏" : "展开右侧工具栏"}
            >
              <MacoIcon name={sidebarOpen ? "panel-left" : "panel-right"} size={16} />
              {sidebarOpen ? "收起面板" : "工具面板"}
            </button>
            <button type="button" className="btn btn-sm" onClick={() => openSidebar("settings")}>
              <MacoIcon name="settings" size={16} />
              设置
            </button>
          </div>
        </header>

        <div className="chat-scroll">
          {messages.length === 0 ? (
            <div className="chat-empty">
              <h2>个人 Agent</h2>
              <p>在设置中配置模型后即可开始对话。</p>
              {models.length === 0 && (
                <button
                  type="button"
                  className="btn btn-primary"
                  onClick={() => openSidebar("settings")}
                >
                  添加模型
                </button>
              )}
            </div>
          ) : (
            messages.map((m: Message, i: number) => (
              <div key={i} className={`msg msg-${m.role}`}>
                <span className="msg-label">{m.role === "user" ? "你" : "助手"}</span>
                <div className="msg-bubble">{m.content}</div>
              </div>
            ))
          )}
          {loading && (
            <div className="msg msg-assistant">
              <span className="msg-label">助手</span>
              <div className="msg-bubble" style={{ opacity: 0.7 }}>
                思考中…
              </div>
            </div>
          )}
          <div ref={chatEndRef} />
        </div>

        {pendingConfirm && (
          <div className="alert alert-warn">
            <strong>工具调用确认</strong>
            <p style={{ margin: "6px 0 0" }}>{pendingConfirm.toolName}</p>
            <div className="alert-actions">
              <button type="button" className="btn btn-sm btn-primary" onClick={() => respondConfirm(true)} disabled={loading}>
                批准
              </button>
              <button type="button" className="btn btn-sm" onClick={() => respondConfirm(false)} disabled={loading}>
                拒绝
              </button>
            </div>
          </div>
        )}

        {pendingElicitation && (
          <div className="alert alert-info">
            <strong>需要 MCP 输入</strong>
            <p style={{ margin: "6px 0 0" }}>{pendingElicitation.message}</p>
            {pendingElicitation.url && (
              <a href={pendingElicitation.url} target="_blank" rel="noreferrer">
                打开链接
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
                提交
              </button>
              <button type="button" className="btn btn-sm" onClick={() => respondElicit("decline")} disabled={loading}>
                拒绝
              </button>
            </div>
          </div>
        )}

        <div className="chat-composer">
          <div className="chat-composer-inner">
            <input
              ref={fileInputRef}
              type="file"
              hidden
              onChange={async (e) => {
                const file = e.target.files?.[0];
                if (!file) return;
                try {
                  const sid = await ensureSession();
                  const art = await uploadArtifact(sid, file);
                  setArtifacts(await listArtifacts(sid));
                  pushMessage({
                    role: "assistant",
                    content: `已上传附件：${art.filename}（${art.mime_type}）`,
                  });
                } catch (err) {
                  pushMessage({ role: "assistant", content: String(err) });
                } finally {
                  e.target.value = "";
                }
              }}
            />
            <button
              type="button"
              className="btn btn-ghost btn-sm btn-icon-only"
              title="上传附件"
              disabled={loading}
              onClick={() => fileInputRef.current?.click()}
            >
              <MacoIcon name="paperclip" size={18} />
            </button>
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
              placeholder="输入消息…（Enter 发送，Shift+Enter 换行）"
              disabled={loading}
            />
            <button type="button" className="btn btn-primary" onClick={send} disabled={loading || !input.trim()}>
              发送
            </button>
          </div>
        </div>
      </div>

      {sidebarOpen && (
        <div className="sidebar-column">
          <div
            className="sidebar-resizer"
            role="separator"
            aria-orientation="vertical"
            aria-label="调节侧栏宽度"
            aria-valuenow={sidebarWidth}
            aria-valuemin={SIDEBAR_WIDTH_MIN}
            aria-valuemax={SIDEBAR_WIDTH_MAX}
            title="拖拽调节宽度，双击恢复默认"
            onPointerDown={onSidebarResizeStart}
            onDoubleClick={resetSidebarWidth}
          />
          <aside className="app-sidebar">
        <nav className="sidebar-nav" aria-label="侧栏导航">
          {tabs.map((t) => (
            <button
              key={t.id}
              type="button"
              className={`sidebar-tab ${sidebarTab === t.id ? "active" : ""}`}
              onClick={() => openSidebar(t.id)}
              title={t.desc}
            >
              <span className="sidebar-tab-icon">
                <MacoIcon name={t.icon} size={18} />
              </span>
              <span className="sidebar-tab-label">{t.label}</span>
            </button>
          ))}
        </nav>

        <div className="sidebar-body">
          <div className="sidebar-header">
            <div className="sidebar-header-text">
              <h2>{activeTab.label}</h2>
              <p>{activeTab.desc}</p>
            </div>
            <button
              type="button"
              className="btn btn-ghost btn-icon sidebar-close"
              onClick={closeSidebar}
              title="收起工具栏"
              aria-label="收起工具栏"
            >
              <MacoIcon name="x" size={16} />
            </button>
          </div>

          <div className="sidebar-panel">
          {sidebarTab === "sessions" && (
            <div className="panel-section">
              {sessions.length === 0 ? (
                <p className="panel-empty">暂无会话，发送第一条消息即可创建。</p>
              ) : (
                sessions.map((s) => (
                  <div key={s.session_id} className="session-row">
                    <button
                      type="button"
                      className={`panel-card panel-card--clickable ${sessionId === s.session_id ? "active" : ""}`}
                      onClick={() => loadSession(s.session_id, s.model_id, s.project_root)}
                    >
                      <span className="session-title">{s.title ?? s.session_id.slice(0, 8)}</span>
                      {s.project_root && (
                        <div className="model-meta session-project" title={s.project_root}>
                          {s.project_root}
                        </div>
                      )}
                      <div className="model-meta">{s.updated_at}</div>
                    </button>
                    <button
                      type="button"
                      className="btn btn-sm btn-ghost session-delete"
                      title="删除会话"
                      aria-label="删除会话"
                      onClick={async () => {
                        if (!confirm("确定删除该会话？")) return;
                        await deleteSession(s.session_id);
                        if (sessionId === s.session_id) {
                          reset();
                          setLastSessionId(null);
                          setArtifacts([]);
                        }
                        setSessions(await listSessions());
                      }}
                    >
                      ×
                    </button>
                  </div>
                ))
              )}
              <div className="panel-actions">
                <button
                  type="button"
                  className="btn btn-sm btn-primary btn-block"
                  onClick={() => {
                    reset();
                    setLastSessionId(null);
                    setArtifacts([]);
                    setPlan("");
                    setTodos([]);
                    setProjectRootDraft("");
                  }}
                >
                  新对话
                </button>
              </div>
              <div className="panel-section" style={{ marginTop: 16 }}>
                <h3>项目路径</h3>
                <p className="panel-empty" style={{ paddingTop: 0 }}>
                  选择本地项目文件夹后，bash 默认在该目录执行，并自动加入 MCP filesystem 允许列表。
                  {!sessionId && " 可在发首条消息前选择，新建会话时会一并保存。"}
                </p>
                {projectRootDraft ? (
                  <div className="project-path-display" title={projectRootDraft}>
                    {projectRootDraft}
                  </div>
                ) : (
                  <p className="project-path-empty">未选择项目文件夹</p>
                )}
                <div className="panel-actions">
                  <button
                    type="button"
                    className="btn btn-sm btn-primary"
                    onClick={pickProjectFolder}
                    disabled={pickingFolder}
                  >
                    {pickingFolder ? "选择中…" : "选择文件夹"}
                  </button>
                  {projectRootDraft && (
                    <button type="button" className="btn btn-sm" onClick={clearProjectRoot}>
                      清除
                    </button>
                  )}
                </div>
              </div>
            </div>
          )}

          {sidebarTab === "tasks" && (
            <>
              <div className="panel-section">
                <h3>文件产物</h3>
                {artifacts.length === 0 ? (
                  <p className="panel-empty">Agent 执行时写入的文件会出现在这里，点击可预览内容。</p>
                ) : (
                  artifacts.map((a) => (
                    <button
                      key={a.id}
                      type="button"
                      className="artifact-card"
                      onClick={() => sessionId && setPreviewArtifactItem(a)}
                      disabled={!sessionId}
                    >
                      <strong>{a.filename}</strong>
                      <div className="model-meta">
                        {a.mime_type} · {(a.size_bytes / 1024).toFixed(1)} KB
                      </div>
                    </button>
                  ))
                )}
              </div>
              <div className="panel-section">
                <h3>计划</h3>
                <div className="panel-card">
                  <pre className="panel-pre">{plan || "暂无计划"}</pre>
                </div>
              </div>
              <div className="panel-section">
                <h3>待办</h3>
                {todos.length === 0 ? (
                  <p className="panel-empty">暂无待办</p>
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
                        <option value="pending">待处理</option>
                        <option value="in_progress">进行中</option>
                        <option value="completed">已完成</option>
                      </select>
                    </div>
                  ))
                )}
              </div>
            </>
          )}

          {sidebarTab === "skills" && <SkillsPanel />}

          {sidebarTab === "memory" && (
            <div className="panel-section">
              <h3>长期记忆</h3>
              {memories.map((m) => (
                <div key={m.id} className="memory-item">
                  <div className="memory-time">{m.timestamp}</div>
                  <div>{m.content}</div>
                </div>
              ))}
              <textarea
                className="chat-input"
                style={{ width: "100%", marginTop: 8, minHeight: 56 }}
                placeholder="添加记忆…"
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
                保存
              </button>
              <div className="input-row" style={{ marginTop: 12 }}>
                <input
                  placeholder="搜索记忆"
                  value={memorySearchQ}
                  onChange={(e) => setMemorySearchQ(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter" && memorySearchQ.trim()) {
                      e.preventDefault();
                      searchMemory(memorySearchQ.trim()).then((r) =>
                        setMemorySearchHits(
                          r.results.map((m, i) => ({
                            id: i,
                            content: m.content,
                            timestamp: r.search_mode,
                          })),
                        ),
                      );
                    }
                  }}
                />
                <button
                  type="button"
                  className="btn btn-sm"
                  disabled={!memorySearchQ.trim()}
                  onClick={async () => {
                    const r = await searchMemory(memorySearchQ.trim());
                    setMemorySearchHits(
                      r.results.map((m, i) => ({
                        id: i,
                        content: m.content,
                        timestamp: r.search_mode,
                      })),
                    );
                  }}
                >
                  搜索
                </button>
              </div>
              {memorySearchHits.length > 0 && (
                <div className="panel-section" style={{ marginTop: 16, marginBottom: 0 }}>
                  <h3>搜索结果</h3>
                  {memorySearchHits.map((m) => (
                    <div key={m.id} className="memory-item">
                      <div className="memory-time">{m.timestamp}</div>
                      <div>{m.content}</div>
                    </div>
                  ))}
                </div>
              )}
              <div className="input-row" style={{ marginTop: 12 }}>
                <input
                  placeholder="按关键词删除"
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
                    setMemorySearchHits([]);
                  }}
                >
                  删除
                </button>
              </div>
            </div>
          )}

          {sidebarTab === "usage" && (
            <div className="panel-section">
              <h3>Token 用量</h3>
              {usage.length === 0 ? (
                <p className="panel-empty">暂无用量数据</p>
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
              <h3>定时任务</h3>
              {jobs.map((j) => (
                <div key={j.id} className="panel-card">
                  <strong>{j.name}</strong>
                  <div className="model-meta">{j.job_type} · {j.status}</div>
                  {j.next_run_at && <div className="model-meta">下次: {j.next_run_at}</div>}
                  <div className="job-card-actions">
                    <button
                      type="button"
                      className="btn btn-sm"
                      onClick={async () => setJobs(await fetchJobs())}
                    >
                      刷新
                    </button>
                    <button
                      type="button"
                      className="btn btn-sm btn-primary"
                      onClick={async () => {
                        await runJobNow(j.id);
                        setJobs(await fetchJobs());
                      }}
                    >
                      立即执行
                    </button>
                  </div>
                </div>
              ))}
              <div className="field">
                <label>新建 ping 任务</label>
                <input
                  value={jobName}
                  onChange={(e) => setJobName(e.target.value)}
                  placeholder="每日 ping"
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
                创建任务
              </button>
            </div>
          )}

          {sidebarTab === "settings" && (
            <>
              <div className="panel-section">
                <h3>外观</h3>
                <p className="panel-empty" style={{ paddingTop: 0 }}>
                  切换界面配色，偏好会保存在本地。
                </p>
                <div className="theme-segment">
                  <button
                    type="button"
                    className={`btn btn-sm ${theme === "light" ? "btn-primary" : ""}`}
                    onClick={() => setTheme("light")}
                  >
                    <MacoIcon name="sun" size={16} />
                    浅色
                  </button>
                  <button
                    type="button"
                    className={`btn btn-sm ${theme === "dark" ? "btn-primary" : ""}`}
                    onClick={() => setTheme("dark")}
                  >
                    <MacoIcon name="moon" size={16} />
                    深色
                  </button>
                </div>
              </div>
              <ModelSettings
                models={models}
                onChange={(list) => {
                  setModels(list);
                  const def = list.find((m) => m.is_default) ?? list[0];
                  if (def) setSelectedModelId(def.id);
                }}
              />
              <McpSettings />
              <ToolPolicySettings />
              <div className="panel-section" style={{ marginTop: 16 }}>
                <h3>API Token</h3>
                {getApiToken() ? (
                  <p className="model-meta">Bearer Token 已保存在本地</p>
                ) : (
                  <p className="model-meta">未设置 Token — 若已开启鉴权请先创建</p>
                )}
                <div className="field">
                  <input
                    placeholder="Token 名称"
                    value={tokenName}
                    onChange={(e) => setTokenName(e.target.value)}
                  />
                </div>
                <button
                  type="button"
                  className="btn btn-sm btn-primary"
                  disabled={!tokenName.trim()}
                  onClick={async () => {
                    const r = await createApiToken(tokenName.trim());
                    setApiToken(r.token);
                    setNewToken(r.token);
                    setTokenName("");
                    setTokenList(await listApiTokens().then((rows) =>
                      rows.map((t) => ({ id: t.id, name: t.name, created_at: t.created_at })),
                    ));
                  }}
                >
                  创建 Token
                </button>
                {newToken && (
                  <pre className="panel-pre" style={{ marginTop: 8, fontSize: "0.75rem" }}>
                    {newToken}
                  </pre>
                )}
                {tokenList.map((t) => (
                  <div key={t.id} className="panel-card" style={{ marginTop: 6 }}>
                    <strong>{t.name}</strong>
                    <div className="model-meta">{t.created_at}</div>
                    <button
                      type="button"
                      className="btn btn-sm"
                      onClick={async () => {
                        await revokeApiToken(t.id);
                        setTokenList(await listApiTokens().then((rows) =>
                          rows.map((x) => ({ id: x.id, name: x.name, created_at: x.created_at })),
                        ));
                      }}
                    >
                      吊销
                    </button>
                  </div>
                ))}
              </div>
              <div className="panel-section" style={{ marginTop: 16 }}>
                <h3>API</h3>
                <a href="/api/docs" target="_blank" rel="noreferrer" className="btn btn-sm">
                  OpenAPI 文档
                </a>
              </div>
            </>
          )}
          </div>
        </div>
          </aside>
        </div>
      )}

      {previewArtifactItem && sessionId && (
        <ArtifactPreviewModal
          sessionId={sessionId}
          artifact={previewArtifactItem}
          onClose={() => setPreviewArtifactItem(null)}
        />
      )}
    </div>
  );
}
