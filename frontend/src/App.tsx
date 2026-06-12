import { useEffect, useRef, useState } from "react";
import {
  addMemory,
  createApiToken,
  createJob,
  createSession,
  deleteJob,
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
  pickProjectDirectory,
  respondElicitation,
  resumeRun,
  revokeApiToken,
  runJobNow,
  searchMemory,
  updateJobEnabled,
  setApiToken,
  streamChat,
  streamRun,
  uploadArtifact,
} from "./api/client";
import { MacoIcon, type MacoIconName } from "./components/Icons";
import { useConfirmDialog } from "./hooks/useConfirmDialog";
import { McpSettings } from "./components/McpSettings";
import { ModelSettings } from "./components/ModelSettings";
import { ChatMessagesPanel } from "./components/ChatMessagesPanel";
import { ChatSessionSidebar } from "./components/ChatSessionSidebar";
import { ElicitationModal } from "./components/ElicitationModal";
import { HitlConfirmModal } from "./components/HitlConfirmModal";
import { RunStatusBar } from "./components/RunStatusBar";
import { SessionProjectBar } from "./components/SessionProjectBar";
import { TasksDock } from "./components/TasksDock";
import { useTasksDockWidth } from "./hooks/useTasksDockWidth";
import { SkillsPanel } from "./components/SkillsPanel";
import { ToolPolicySettings } from "./components/ToolPolicySettings";
import { useStreamingAssistantBuffer } from "./hooks/useStreamingAssistantBuffer";
import { useChatStore } from "./store/chat";
import { applyTheme, getTheme, type Theme, toggleTheme as flipTheme } from "./theme";

type SseEvent = {
  type: string;
  run_id?: string;
  payload?: {
    content?: string;
    message?: string;
    tool_name?: string;
    name?: string;
    args?: Record<string, unknown>;
    elicitation_id?: string;
    request_type?: string;
    url?: string;
    id?: string;
    filename?: string;
    mime_type?: string;
    size_bytes?: number;
  };
};

type ToolTab = "memory" | "skills" | "usage" | "jobs" | "models" | "mcp" | "toolPolicies" | "settings";
type AppView = "chat" | ToolTab;

export default function App() {
  const {
    sessionId,
    setSessionId,
    setMessages,
    pushMessage,
    dropAssistantTurnBeforeTool,
    reset,
    clearMessages,
  } = useChatStore();
  const { appendChunk, flush: flushAssistantStream } = useStreamingAssistantBuffer();
  const [input, setInput] = useState("");
  const [plan, setPlan] = useState("");
  const [todos, setTodos] = useState<Array<{ task_key: string; title: string; status: string }>>([]);
  const [loading, setLoading] = useState(false);
  const [confirmSubmitting, setConfirmSubmitting] = useState(false);
  const [elicitationSubmitting, setElicitationSubmitting] = useState(false);
  const [pendingConfirm, setPendingConfirm] = useState<{
    runId: string;
    toolName: string;
    toolArgs?: Record<string, unknown> | null;
  } | null>(null);
  const [pendingElicitation, setPendingElicitation] = useState<{
    id: string;
    requestType: string;
    message: string;
    url?: string;
  } | null>(null);
  const [elicitationInput, setElicitationInput] = useState("{}");
  const [usage, setUsage] = useState<Array<{ key: string; total_tokens: number; estimated_cost: number }>>([]);
  const [usageError, setUsageError] = useState<string | null>(null);
  const [memories, setMemories] = useState<Array<{ id: number; content: string; timestamp: string }>>([]);
  const [memoryError, setMemoryError] = useState<string | null>(null);
  const [memoryInput, setMemoryInput] = useState("");
  const [memoryDeleteQ, setMemoryDeleteQ] = useState("");
  const [models, setModels] = useState<ModelView[]>([]);
  const [selectedModelId, setSelectedModelId] = useState<string>("");
  const [appView, setAppView] = useState<AppView>("chat");
  const [agentActivity, setAgentActivity] = useState<string | null>(null);
  const [theme, setTheme] = useState<Theme>(getTheme);

  useEffect(() => {
    applyTheme(theme);
  }, [theme]);

  function onToggleTheme() {
    setTheme((current) => flipTheme(current));
  }

  function refreshUsage() {
    return fetchUsageSummary("model")
      .then((rows) => {
        setUsage(
          rows.map((r) => ({
            key: r.key,
            total_tokens: r.total_tokens,
            estimated_cost: r.estimated_cost,
          })),
        );
        setUsageError(null);
      })
      .catch((err: unknown) => {
        setUsage([]);
        setUsageError(err instanceof Error ? err.message : "加载用量失败");
      });
  }

  function refreshMemories() {
    return fetchMemories()
      .then((r) => {
        setMemories(r.items);
        setMemoryError(null);
      })
      .catch((err: unknown) => {
        setMemories([]);
        setMemoryError(err instanceof Error ? err.message : "加载记忆失败");
      });
  }

  function navigateTo(view: AppView) {
    setAppView(view);
    if (view === "chat" && sessionId) {
      refreshTasks(sessionId).catch(() => undefined);
      listArtifacts(sessionId).then(setArtifacts).catch(() => setArtifacts([]));
    }
    if (view === "usage") {
      refreshUsage().catch(() => undefined);
    }
    if (view === "memory") {
      refreshMemories().catch(() => undefined);
    }
  }
  const [jobs, setJobs] = useState<JobRecord[]>([]);
  const [jobName, setJobName] = useState("");
  const { runConfirm, dialog: confirmDialog } = useConfirmDialog();
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
  const [projectRootDraft, setProjectRootDraft] = useState("");
  const [pickingFolder, setPickingFolder] = useState(false);
  const [restored, setRestored] = useState(false);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const chatAbortRef = useRef<AbortController | null>(null);
  const sessionIdRef = useRef<string | null>(sessionId);

  useEffect(() => {
    sessionIdRef.current = sessionId;
  }, [sessionId]);

  const { width: tasksDockWidth, onResizeStart } = useTasksDockWidth();
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
    refreshUsage().catch(() => undefined);
    refreshMemories().catch(() => undefined);
    fetchJobs()
      .then(setJobs)
      .catch(() => setJobs([]));
  }, [sessionId]);

  function handleSse(raw: Record<string, unknown>) {
    const ev = raw as SseEvent & { event_type?: string };
    const type = ev.type ?? ev.event_type;
    const activeSessionId = sessionIdRef.current;
    if (ev.run_id) setActiveRunId(ev.run_id);
    if (type === "text" && ev.payload?.content) {
      appendChunk(ev.payload.content);
    }
    if (type === "tool_call") {
      const toolName = ev.payload?.name ?? "unknown";
      dropAssistantTurnBeforeTool();
      setAgentActivity(`正在调用工具：${toolName}`);
    }
    if (type === "tool_confirm_request" && ev.run_id && ev.payload?.tool_name) {
      dropAssistantTurnBeforeTool();
      setPendingConfirm({
        runId: ev.run_id,
        toolName: ev.payload.tool_name,
        toolArgs: (ev.payload.args as Record<string, unknown> | undefined) ?? null,
      });
      setAgentActivity(`等待确认工具：${ev.payload.tool_name}`);
    }
    if (type === "elicitation_request" && ev.payload?.elicitation_id) {
      setPendingElicitation({
        id: ev.payload.elicitation_id,
        requestType: ev.payload.request_type ?? "form",
        message: ev.payload.message ?? "需要补充输入",
        url: ev.payload.url,
      });
      setAgentActivity("等待 MCP 补充输入");
    }
    if (type === "error" && ev.payload?.message) {
      flushAssistantStream();
      pushMessage({ role: "assistant", content: `错误：${ev.payload.message}` });
      setLoading(false);
      setAgentActivity(null);
      setActiveRunId(null);
      setPendingConfirm(null);
      setPendingElicitation(null);
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
    if (type === "tasks_updated" && activeSessionId) {
      refreshTasks(activeSessionId).catch(() => undefined);
    }
    if (type === "awaiting_user") {
      setLoading(true);
      setAgentActivity("等待你的确认…");
    }
    if (type === "done") {
      flushAssistantStream();
      setActiveRunId(null);
      setLoading(false);
      setAgentActivity(null);
      setPendingConfirm(null);
      const sid = activeSessionId ?? sessionIdRef.current;
      if (sid) {
        refreshTasks(sid).catch(() => undefined);
        listArtifacts(sid).then(setArtifacts).catch(() => undefined);
      }
    }
  }

  async function ensureSession() {
    if (sessionId) return sessionId;
    const modelId = selectedModelId || defaultModel?.id;
    const root = projectRootDraft.trim() || undefined;
    const s = await createSession(undefined, modelId, root);
    sessionIdRef.current = s.session_id;
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
    sessionIdRef.current = sid;
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
            const tool = run.pending_tools[0];
            setPendingConfirm({
              runId: active.run_id,
              toolName: tool.name,
              toolArgs: tool.args ?? null,
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

  async function renameSession(sid: string, title: string) {
    await updateSession(sid, { title: title.trim() });
    setSessions(await listSessions());
  }

  async function deleteSessionById(sid: string) {
    const previous = sessions;
    setSessions((prev) => prev.filter((s) => s.session_id !== sid));
    if (sessionId === sid) {
      sessionIdRef.current = null;
      reset();
      setLastSessionId(null);
      setArtifacts([]);
      setPlan("");
      setTodos([]);
      setProjectRootDraft("");
      clearMessages();
    }
    try {
      await deleteSession(sid);
      setSessions(await listSessions());
    } catch (e) {
      setSessions(previous);
      pushMessage({ role: "assistant", content: `删除会话失败：${e}` });
      throw e;
    }
  }

  function startNewChat() {
    reset();
    setLastSessionId(null);
    setArtifacts([]);
    setPlan("");
    setTodos([]);
    setProjectRootDraft("");
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
        content: "请先在「模型」中配置 API（API Key + Base URL + Model ID）",
      });
      navigateTo("models");
      return;
    }
    setLoading(true);
    setAgentActivity("思考中…");
    try {
      const sid = await ensureSession();
      const isFirstMessage = useChatStore.getState().messages.length === 0;
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
      flushAssistantStream();
      pushMessage({ role: "assistant", content: String(e) });
    } finally {
      flushAssistantStream();
      setLoading(false);
      setAgentActivity(null);
    }
  }

  async function reconnectActiveStream(sid: string) {
    const active = await fetchActiveRun(sid);
    if (!active.run_id) return;
    setActiveRunId(active.run_id);
    await streamRun(sid, active.run_id, handleSse);
  }

  async function respondElicit(action: "accept" | "decline" | "cancel") {
    if (!pendingElicitation || elicitationSubmitting) return;
    setElicitationSubmitting(true);
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
      setElicitationSubmitting(false);
    }
  }

  async function stopRun() {
    if (!sessionId) return;
    if (!loading && !activeRunId && !pendingConfirm && !pendingElicitation) return;
    chatAbortRef.current?.abort();
    try {
      await interruptChat(sessionId);
      setActiveRunId(null);
      setPendingConfirm(null);
      setPendingElicitation(null);
    } catch (e) {
      pushMessage({ role: "assistant", content: String(e) });
    } finally {
      setLoading(false);
    }
  }

  async function respondConfirm(approved: boolean) {
    if (!sessionId || !pendingConfirm || confirmSubmitting) return;
    setConfirmSubmitting(true);
    try {
      const mode = await resumeRun(
        sessionId,
        pendingConfirm.runId,
        approved,
        handleSse,
      );
      setPendingConfirm(null);
      setAgentActivity(approved ? "工具已批准，继续执行…" : null);
      if (mode === "stream") {
        setLoading(true);
        await refreshTasks(sessionId);
        setLoading(false);
      }
    } catch (e) {
      pushMessage({ role: "assistant", content: String(e) });
      setPendingConfirm(null);
      setLoading(false);
      setAgentActivity(null);
    } finally {
      setConfirmSubmitting(false);
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

  const toolbarItems: { id: AppView; label: string; icon: MacoIconName; desc: string }[] = [
    { id: "chat", label: "会话", icon: "sessions", desc: "当前对话与历史会话" },
    { id: "memory", label: "记忆", icon: "memory", desc: "长期记忆读写" },
    { id: "skills", label: "技能", icon: "skills", desc: "本地 Skill 文件" },
    { id: "usage", label: "用量", icon: "usage", desc: "Token 与费用统计" },
    { id: "jobs", label: "调度", icon: "jobs", desc: "定时后台任务" },
    { id: "models", label: "模型", icon: "models", desc: "LLM 提供商与 API Key" },
    { id: "mcp", label: "MCP", icon: "mcp", desc: "MCP 服务连接与重载" },
    { id: "toolPolicies", label: "策略", icon: "toolPolicies", desc: "工具 HITL 确认策略" },
    { id: "settings", label: "设置", icon: "settings", desc: "外观、鉴权与 API 文档" },
  ];

  const activeTool = toolbarItems.find((t) => t.id === appView) ?? toolbarItems[0];

  const runNeedsAttention = Boolean(pendingConfirm || pendingElicitation);
  const runInProgress = loading || Boolean(activeRunId);
  const showRunActivity = runNeedsAttention || runInProgress;

  const runStatusMessage = pendingConfirm
    ? `工具待确认：${pendingConfirm.toolName}`
    : pendingElicitation
      ? "等待 MCP 补充输入"
      : "Agent 正在运行…";

  return (
    <div className="app-shell">
      <nav className="app-toolbar-left" aria-label="工具栏">
        {toolbarItems.map((t) => (
          <button
            key={t.id}
            type="button"
            className={`toolbar-tab ${appView === t.id ? "active" : ""}`}
            onClick={() => navigateTo(t.id)}
            title={t.desc}
          >
            <span className="toolbar-tab-icon">
              <MacoIcon name={t.icon} size={18} />
              {t.id === "chat" && showRunActivity && (
                <span
                  className={`toolbar-run-dot ${runNeedsAttention ? "warn" : "active"}`}
                  title={runStatusMessage}
                  aria-hidden
                />
              )}
            </span>
            <span className="toolbar-tab-label">{t.label}</span>
          </button>
        ))}
      </nav>

      <div className="app-workspace">
        {appView !== "chat" && showRunActivity && (
          <RunStatusBar
            message={runStatusMessage}
            onGoToChat={() => navigateTo("chat")}
            onStop={() => void stopRun()}
            stopping={loading}
          />
        )}
        {appView === "chat" ? (
          <div
            className="chat-workspace"
            style={{ gridTemplateColumns: `minmax(0, 1fr) ${tasksDockWidth}px` }}
          >
            <div className="chat-main">
              <ChatSessionSidebar
                sessions={sessions}
                activeSessionId={sessionId}
                onSelect={(s) => loadSession(s.session_id, s.model_id, s.project_root)}
                onNewChat={startNewChat}
                onDelete={deleteSessionById}
                onRename={renameSession}
              />
              <div className="chat-column">
                <header className="app-topbar">
                  <div className="app-topbar-title">
                    <div className="app-logo">ma<span>co</span></div>
                    {currentSession ? (
                      <span
                        className="app-topbar-session"
                        title={currentSession.title ?? currentSession.session_id}
                      >
                        {currentSession.title ?? currentSession.session_id.slice(0, 8)}
                      </span>
                    ) : (
                      <span className="app-topbar-session app-topbar-session--muted">新对话</span>
                    )}
                  </div>
                  <select
                    className="model-select"
                    value={selectedModelId}
                    onChange={(e) => setSelectedModelId(e.target.value)}
                    title="对话模型"
                  >
                    {models.length === 0 ? (
                      <option value="">无模型 — 请打开「模型」</option>
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
                  </div>
                </header>

                <SessionProjectBar
                  projectRootDraft={projectRootDraft}
                  pickingFolder={pickingFolder}
                  hasSession={Boolean(sessionId)}
                  onPickFolder={pickProjectFolder}
                  onClear={clearProjectRoot}
                />

                <ChatMessagesPanel
                  loading={loading}
                  agentActivity={agentActivity}
                  modelsEmpty={models.length === 0}
                  onOpenModels={() => navigateTo("models")}
                />

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
                    <button
                      type="button"
                      className="btn btn-primary"
                      onClick={send}
                      disabled={loading || !input.trim()}
                    >
                      发送
                    </button>
                  </div>
                </div>
              </div>
            </div>
            <div className="tasks-dock-column">
              <div
                className="tasks-dock-resizer"
                role="separator"
                aria-orientation="vertical"
                aria-label="调整任务面板宽度"
                onMouseDown={onResizeStart}
              />
              <TasksDock
                plan={plan}
                todos={todos}
                artifacts={artifacts}
                sessionId={sessionId}
                busy={loading || !!activeRunId}
              />
            </div>
          </div>
        ) : (
          <div className="tool-workspace">
            <div className="tool-workspace-header">
              <h2>{activeTool.label}</h2>
              <p>{activeTool.desc}</p>
            </div>
            <div className="tool-workspace-panel">
          {appView === "memory" && (
            <div className="panel-section">
              <h3>长期记忆</h3>
              {memoryError ? (
                <p className="panel-empty panel-error">{memoryError}</p>
              ) : memories.length === 0 ? (
                <p className="panel-empty">暂无记忆。可在下方手动添加，或由 Agent 在对话中写入长期记忆。</p>
              ) : null}
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
                  await refreshMemories();
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
                    await refreshMemories();
                    setMemorySearchHits([]);
                  }}
                >
                  删除
                </button>
              </div>
            </div>
          )}

          {appView === "skills" && <SkillsPanel />}

          {appView === "usage" && (
            <div className="panel-section">
              <h3>Token 用量</h3>
              {usageError ? (
                <p className="panel-empty panel-error">{usageError}</p>
              ) : usage.length === 0 ? (
                <p className="panel-empty">暂无用量数据。完成至少一次对话后，模型调用的 token 会汇总显示在这里。</p>
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

          {appView === "jobs" && (
            <div className="panel-section">
              <h3>定时任务</h3>
              {jobs.length === 0 ? (
                <p className="panel-empty">暂无定时任务。可在下方创建 hourly ping 任务。</p>
              ) : null}
              {jobs.map((j) => {
                const paused = j.enabled === 0;
                return (
                <div key={j.id} className={`panel-card${paused ? " panel-card--disabled" : ""}`}>
                  <div className="job-card-header">
                    <strong>{j.name}</strong>
                    {paused ? <span className="badge">已暂停</span> : null}
                  </div>
                  <div className="model-meta">
                    {j.job_type} · {j.schedule ?? "手动"} · {j.status}
                  </div>
                  {j.next_run_at && !paused ? (
                    <div className="model-meta">下次: {j.next_run_at}</div>
                  ) : null}
                  {j.last_run_at ? (
                    <div className="model-meta">上次: {j.last_run_at}</div>
                  ) : null}
                  <div className="job-card-actions">
                    <button
                      type="button"
                      className="btn btn-sm btn-primary"
                      disabled={paused}
                      onClick={async () => {
                        await runJobNow(j.id);
                        setJobs(await fetchJobs());
                      }}
                    >
                      立即执行
                    </button>
                    <button
                      type="button"
                      className="btn btn-sm"
                      onClick={async () => {
                        await updateJobEnabled(j.id, paused);
                        setJobs(await fetchJobs());
                      }}
                    >
                      {paused ? "恢复" : "暂停"}
                    </button>
                    <button
                      type="button"
                      className="btn btn-sm"
                      onClick={() => {
                        runConfirm(
                          {
                            title: "删除定时任务",
                            description: `确定删除任务「${j.name}」？此操作不可恢复。`,
                            confirmLabel: "删除",
                            tone: "danger",
                          },
                          async () => {
                            await deleteJob(j.id);
                            setJobs(await fetchJobs());
                          },
                        );
                      }}
                    >
                      删除
                    </button>
                  </div>
                </div>
              );
              })}
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

          {appView === "models" && (
            <ModelSettings
              models={models}
              onChange={(list) => {
                setModels(list);
                const def = list.find((m) => m.is_default) ?? list[0];
                if (def) setSelectedModelId(def.id);
              }}
            />
          )}

          {appView === "mcp" && <McpSettings />}

          {appView === "toolPolicies" && <ToolPolicySettings />}

          {appView === "settings" && (
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
        )}
      </div>

      {confirmDialog}

      {pendingConfirm && (
        <HitlConfirmModal
          toolName={pendingConfirm.toolName}
          toolArgs={pendingConfirm.toolArgs}
          submitting={confirmSubmitting}
          onApprove={() => void respondConfirm(true)}
          onReject={() => void respondConfirm(false)}
        />
      )}

      {pendingElicitation && (
        <ElicitationModal
          message={pendingElicitation.message}
          requestType={pendingElicitation.requestType}
          url={pendingElicitation.url}
          input={elicitationInput}
          submitting={elicitationSubmitting}
          onInputChange={setElicitationInput}
          onAccept={() => void respondElicit("accept")}
          onDecline={() => void respondElicit("decline")}
        />
      )}
    </div>
  );
}
