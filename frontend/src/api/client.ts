const API = "/api";
const TOKEN_KEY = "maco_api_token";
const LAST_SESSION_KEY = "maco_last_session";
const LAST_MODEL_KEY = "maco_last_model";
const LAST_PERMISSION_MODE_KEY = "maco_last_permission_mode";
const SIDEBAR_VISIBLE_KEY = "maco_sidebar_visible";
const SIDEBAR_WIDTH_KEY = "maco_sidebar_width";

export const SIDEBAR_WIDTH_MIN = 360;
export const SIDEBAR_WIDTH_DEFAULT = 480;
export const SIDEBAR_WIDTH_MAX = 720;

export function getSidebarVisible(): boolean {
  const raw = localStorage.getItem(SIDEBAR_VISIBLE_KEY);
  return raw === null ? true : raw === "1";
}

export function persistSidebarVisible(visible: boolean): void {
  localStorage.setItem(SIDEBAR_VISIBLE_KEY, visible ? "1" : "0");
}

export function getSidebarWidth(): number {
  const raw = localStorage.getItem(SIDEBAR_WIDTH_KEY);
  if (!raw) return SIDEBAR_WIDTH_DEFAULT;
  const n = Number.parseInt(raw, 10);
  if (!Number.isFinite(n)) return SIDEBAR_WIDTH_DEFAULT;
  return Math.min(SIDEBAR_WIDTH_MAX, Math.max(SIDEBAR_WIDTH_MIN, n));
}

export function persistSidebarWidth(width: number): void {
  const clamped = Math.min(SIDEBAR_WIDTH_MAX, Math.max(SIDEBAR_WIDTH_MIN, Math.round(width)));
  localStorage.setItem(SIDEBAR_WIDTH_KEY, String(clamped));
}

const TASKS_DOCK_WIDTH_KEY = "maco_tasks_dock_width";

export const TASKS_DOCK_WIDTH_MIN = 380;
export const TASKS_DOCK_WIDTH_DEFAULT = 440;
export const TASKS_DOCK_WIDTH_MAX = 680;

export function getTasksDockWidth(): number {
  const raw = localStorage.getItem(TASKS_DOCK_WIDTH_KEY);
  if (!raw) return TASKS_DOCK_WIDTH_DEFAULT;
  const n = Number.parseInt(raw, 10);
  if (!Number.isFinite(n)) return TASKS_DOCK_WIDTH_DEFAULT;
  return Math.min(TASKS_DOCK_WIDTH_MAX, Math.max(TASKS_DOCK_WIDTH_MIN, n));
}

export function persistTasksDockWidth(width: number): void {
  const clamped = Math.min(
    TASKS_DOCK_WIDTH_MAX,
    Math.max(TASKS_DOCK_WIDTH_MIN, Math.round(width)),
  );
  localStorage.setItem(TASKS_DOCK_WIDTH_KEY, String(clamped));
}

export function getLastSessionId(): string | null {
  return localStorage.getItem(LAST_SESSION_KEY);
}

export function setLastSessionId(id: string | null) {
  if (id) localStorage.setItem(LAST_SESSION_KEY, id);
  else localStorage.removeItem(LAST_SESSION_KEY);
}

export function getLastModelId(): string | null {
  return localStorage.getItem(LAST_MODEL_KEY);
}

export function setLastModelId(id: string | null) {
  if (id) localStorage.setItem(LAST_MODEL_KEY, id);
  else localStorage.removeItem(LAST_MODEL_KEY);
}

export type AgentPermissionMode =
  | "request_approval"
  | "auto_approve"
  | "full_access";

export function getLastPermissionMode(): AgentPermissionMode {
  const raw = localStorage.getItem(LAST_PERMISSION_MODE_KEY);
  if (raw === "auto_approve" || raw === "full_access" || raw === "request_approval") {
    return raw;
  }
  return "request_approval";
}

export function setLastPermissionMode(mode: AgentPermissionMode) {
  localStorage.setItem(LAST_PERMISSION_MODE_KEY, mode);
}

export type ModelView = {
  id: string;
  name: string;
  provider: "openai" | "anthropic" | "gemini" | "openrouter";
  model_id: string;
  base_url: string | null;
  api_key_env: string;
  is_default: boolean;
  enabled: boolean;
  config: string;
  has_api_key: boolean;
  api_key_preview: string | null;
};

export type JobRecord = {
  id: string;
  name: string;
  job_type: string;
  schedule: string | null;
  status: string;
  last_run_at: string | null;
  next_run_at: string | null;
  enabled: number;
};

export function getApiToken(): string | null {
  return localStorage.getItem(TOKEN_KEY);
}

export function setApiToken(token: string | null) {
  if (token) localStorage.setItem(TOKEN_KEY, token);
  else localStorage.removeItem(TOKEN_KEY);
}

function authHeaders(extra?: HeadersInit): HeadersInit {
  const headers: Record<string, string> = {
    "Content-Type": "application/json",
  };
  const token = getApiToken();
  if (token) headers.Authorization = `Bearer ${token}`;
  return { ...headers, ...(extra as Record<string, string> | undefined) };
}

export async function createApiToken(name: string) {
  const res = await fetch(`${API}/auth/tokens`, {
    method: "POST",
    headers: authHeaders(),
    body: JSON.stringify({ name, scopes: ["admin", "chat", "memory", "*"] }),
  });
  if (!res.ok) throw new Error(await res.text());
  return res.json() as Promise<{ token: string; id: string; name: string }>;
}

export async function fetchModels() {
  const res = await fetch(`${API}/models`, { headers: authHeaders() });
  if (!res.ok) throw new Error(await res.text());
  return res.json() as Promise<ModelView[]>;
}

export async function upsertModel(
  body: {
    name: string;
    provider: string;
    model_id: string;
    base_url?: string;
    api_key?: string;
    api_key_env?: string;
    is_default?: boolean;
    enabled?: boolean;
  },
  id?: string,
) {
  const res = await fetch(id ? `${API}/models/${id}` : `${API}/models`, {
    method: id ? "PATCH" : "POST",
    headers: authHeaders(),
    body: JSON.stringify(body),
  });
  if (!res.ok) throw new Error(await res.text());
  return res.json() as Promise<ModelView>;
}

export async function deleteModel(id: string) {
  const res = await fetch(`${API}/models/${id}`, {
    method: "DELETE",
    headers: authHeaders(),
  });
  if (!res.ok) throw new Error(await res.text());
}

function normalizePermissionMode(
  mode: string | null | undefined,
): AgentPermissionMode {
  if (mode === "auto_approve" || mode === "full_access") return mode;
  return "request_approval";
}

export async function createSession(
  title?: string,
  modelId?: string,
  projectRoot?: string,
  permissionMode?: AgentPermissionMode,
) {
  const res = await fetch(`${API}/sessions`, {
    method: "POST",
    headers: authHeaders(),
    body: JSON.stringify({
      title: title ?? "New chat",
      model_id: modelId,
      project_root: projectRoot?.trim() || undefined,
      permission_mode: permissionMode ?? "request_approval",
    }),
  });
  if (!res.ok) throw new Error(await res.text());
  return res.json() as Promise<{ session_id: string }>;
}

async function consumeSse(
  res: Response,
  onEvent: (data: Record<string, unknown>) => void,
) {
  if (!res.ok || !res.body) throw new Error(await res.text());
  const reader = res.body.getReader();
  const decoder = new TextDecoder();
  let buffer = "";
  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    buffer += decoder.decode(value, { stream: true });
    const parts = buffer.split("\n\n");
    buffer = parts.pop() ?? "";
    for (const part of parts) {
      const line = part.trim();
      if (!line.startsWith("data:")) continue;
      const json = line.slice(5).trim();
      if (json) onEvent(JSON.parse(json));
    }
  }
}

export async function streamChat(
  sessionId: string,
  message: string,
  onEvent: (data: Record<string, unknown>) => void,
  modelId?: string,
  signal?: AbortSignal,
) {
  const res = await fetch(`${API}/chat`, {
    method: "POST",
    headers: authHeaders(),
    body: JSON.stringify({
      session_id: sessionId,
      message,
      model_id: modelId,
    }),
    signal,
  });
  await consumeSse(res, onEvent);
}

export async function resumeRun(
  sessionId: string,
  runId: string,
  approved: boolean,
  onEvent: (data: Record<string, unknown>) => void,
  note?: string,
): Promise<"in_place" | "stream"> {
  const res = await fetch(`${API}/sessions/${sessionId}/runs/${runId}/resume`, {
    method: "POST",
    headers: authHeaders(),
    body: JSON.stringify({ approved, note }),
  });
  if (!res.ok) throw new Error(await res.text());
  const contentType = res.headers.get("content-type") ?? "";
  if (contentType.includes("application/json")) {
    return "in_place";
  }
  await consumeSse(res, onEvent);
  return "stream";
}

export async function fetchPlan(sessionId: string) {
  const res = await fetch(`${API}/sessions/${sessionId}/plan`, {
    headers: authHeaders(),
  });
  if (res.status === 404) return null;
  if (!res.ok) throw new Error(await res.text());
  return res.json() as Promise<{ content: string; version: number }>;
}

export async function fetchTodos(sessionId: string) {
  const res = await fetch(`${API}/sessions/${sessionId}/todos`, {
    headers: authHeaders(),
  });
  if (!res.ok) throw new Error(await res.text());
  return res.json() as Promise<
    Array<{ task_key: string; title: string; status: string }>
  >;
}

export async function fetchUsageSummary(groupBy: "model" | "day" | "session" = "model") {
  const res = await fetch(`${API}/usage/summary?group_by=${groupBy}`, {
    headers: authHeaders(),
  });
  if (!res.ok) throw new Error(await res.text());
  return res.json() as Promise<
    Array<{
      key: string;
      total_tokens: number;
      estimated_cost: number;
    }>
  >;
}

export async function respondElicitation(
  elicitationId: string,
  action: "accept" | "decline" | "cancel",
  content?: Record<string, unknown>,
) {
  const res = await fetch(`${API}/elicitation/${elicitationId}/respond`, {
    method: "POST",
    headers: authHeaders(),
    body: JSON.stringify({ action, content }),
  });
  if (!res.ok) throw new Error(await res.text());
  return res.json() as Promise<{ id: string; fulfilled: boolean }>;
}

export async function fetchMemories(limit = 50) {
  const res = await fetch(`${API}/memory?limit=${limit}`, {
    headers: authHeaders(),
  });
  if (!res.ok) throw new Error(await res.text());
  return res.json() as Promise<{
    items: Array<{
      id: number;
      content: string;
      timestamp: string;
    }>;
  }>;
}

export async function addMemory(content: string) {
  const res = await fetch(`${API}/memory`, {
    method: "POST",
    headers: authHeaders(),
    body: JSON.stringify({ content }),
  });
  if (!res.ok) throw new Error(await res.text());
}

export async function deleteMemories(query: string) {
  const res = await fetch(`${API}/memory?q=${encodeURIComponent(query)}`, {
    method: "DELETE",
    headers: authHeaders(),
  });
  if (!res.ok) throw new Error(await res.text());
  return res.json() as Promise<{ deleted: number }>;
}

export function exportSessionUrl(sessionId: string) {
  return `${API}/sessions/${sessionId}/export`;
}

export async function fetchJobs() {
  const res = await fetch(`${API}/jobs`, { headers: authHeaders() });
  if (!res.ok) throw new Error(await res.text());
  return res.json() as Promise<JobRecord[]>;
}

export async function createJob(body: {
  name: string;
  job_type: string;
  schedule?: string;
  payload?: string;
  run_at?: string;
}) {
  const res = await fetch(`${API}/jobs`, {
    method: "POST",
    headers: authHeaders(),
    body: JSON.stringify(body),
  });
  if (!res.ok) throw new Error(await res.text());
  return res.json() as Promise<JobRecord>;
}

export async function runJobNow(id: string) {
  const res = await fetch(`${API}/jobs/${id}/run`, {
    method: "POST",
    headers: authHeaders(),
  });
  if (!res.ok) throw new Error(await res.text());
  return res.json() as Promise<JobRecord>;
}

export async function updateJobEnabled(id: string, enabled: boolean) {
  const res = await fetch(`${API}/jobs/${id}`, {
    method: "PATCH",
    headers: authHeaders(),
    body: JSON.stringify({ enabled }),
  });
  if (!res.ok) throw new Error(await res.text());
}

export async function deleteJob(id: string) {
  const res = await fetch(`${API}/jobs/${id}`, {
    method: "DELETE",
    headers: authHeaders(),
  });
  if (!res.ok) throw new Error(await res.text());
}

export async function patchTodo(
  sessionId: string,
  taskKey: string,
  status: string,
) {
  const res = await fetch(`${API}/sessions/${sessionId}/todos/${taskKey}`, {
    method: "PATCH",
    headers: authHeaders(),
    body: JSON.stringify({ status }),
  });
  if (!res.ok) throw new Error(await res.text());
  return res.json();
}

export type SessionMeta = {
  session_id: string;
  title: string | null;
  model_id: string | null;
  project_root: string | null;
  permission_mode: AgentPermissionMode;
  status: string;
  created_at: string;
  updated_at: string;
};

export type McpServerRecord = {
  id: string;
  name: string;
  transport: "stdio" | "sse";
  command: string | null;
  args: string;
  url: string | null;
  env: string;
  enabled: number;
};

export type ApiTokenListItem = {
  id: string;
  name: string;
  scopes: string;
  expires_at: string | null;
  created_at: string;
};

export function visibleSessions(rows: SessionMeta[]): SessionMeta[] {
  return rows
    .filter((s) => s.status !== "deleted" && s.status !== "pending_delete")
    .map((s) => ({
      ...s,
      permission_mode: normalizePermissionMode(s.permission_mode),
    }));
}

export async function listSessions() {
  const res = await fetch(`${API}/sessions`, { headers: authHeaders() });
  if (!res.ok) throw new Error(await res.text());
  const rows = (await res.json()) as SessionMeta[];
  return visibleSessions(rows);
}

export async function searchMemory(q: string) {
  const res = await fetch(`${API}/memory/search?q=${encodeURIComponent(q)}`, {
    headers: authHeaders(),
  });
  if (!res.ok) throw new Error(await res.text());
  return res.json() as Promise<{
    search_mode: string;
    results: Array<{ content: string; score?: number | null }>;
  }>;
}

export async function uploadArtifact(sessionId: string, file: File) {
  const form = new FormData();
  form.append("file", file);
  const headers: Record<string, string> = {};
  const token = getApiToken();
  if (token) headers.Authorization = `Bearer ${token}`;
  const res = await fetch(`${API}/sessions/${sessionId}/artifacts`, {
    method: "POST",
    headers,
    body: form,
  });
  if (!res.ok) throw new Error(await res.text());
  return res.json() as Promise<ArtifactRecord>;
}

export async function listApiTokens() {
  const res = await fetch(`${API}/auth/tokens`, { headers: authHeaders() });
  if (!res.ok) throw new Error(await res.text());
  return res.json() as Promise<ApiTokenListItem[]>;
}

export async function revokeApiToken(id: string) {
  const res = await fetch(`${API}/auth/tokens/${id}`, {
    method: "DELETE",
    headers: authHeaders(),
  });
  if (!res.ok) throw new Error(await res.text());
}

export async function interruptChat(sessionId: string) {
  const res = await fetch(`${API}/chat/${sessionId}/interrupt`, {
    method: "POST",
    headers: authHeaders(),
  });
  if (!res.ok) throw new Error(await res.text());
  return res.json() as Promise<{
    session_id: string;
    interrupted: boolean;
    run_id: string | null;
  }>;
}

export async function fetchActiveRun(sessionId: string) {
  const res = await fetch(`${API}/sessions/${sessionId}/runs/active`, {
    headers: authHeaders(),
  });
  if (!res.ok) throw new Error(await res.text());
  return res.json() as Promise<{ session_id: string; run_id: string | null }>;
}

export async function streamRun(
  sessionId: string,
  runId: string,
  onEvent: (data: Record<string, unknown>) => void,
) {
  const res = await fetch(`${API}/sessions/${sessionId}/runs/${runId}/stream`, {
    headers: authHeaders(),
  });
  await consumeSse(res, onEvent);
}

export async function listMcpServers() {
  const res = await fetch(`${API}/mcp/servers`, { headers: authHeaders() });
  if (!res.ok) throw new Error(await res.text());
  return res.json() as Promise<McpServerRecord[]>;
}

export async function createMcpServer(body: {
  name: string;
  transport: string;
  command?: string;
  args?: string;
  url?: string;
  env?: string;
}) {
  const res = await fetch(`${API}/mcp/servers`, {
    method: "POST",
    headers: authHeaders(),
    body: JSON.stringify(body),
  });
  if (!res.ok) throw new Error(await res.text());
  return res.json() as Promise<McpServerRecord>;
}

export async function updateMcpServer(
  id: string,
  body: {
    name: string;
    transport: string;
    command?: string;
    args?: string;
    url?: string;
    env?: string;
    enabled?: boolean;
  },
) {
  const res = await fetch(`${API}/mcp/servers/${id}`, {
    method: "PATCH",
    headers: authHeaders(),
    body: JSON.stringify(body),
  });
  if (!res.ok) throw new Error(await res.text());
  return res.json() as Promise<McpServerRecord>;
}

export async function deleteMcpServer(id: string) {
  const res = await fetch(`${API}/mcp/servers/${id}`, {
    method: "DELETE",
    headers: authHeaders(),
  });
  if (!res.ok) throw new Error(await res.text());
}

export async function reloadMcpPool() {
  const res = await fetch(`${API}/mcp/reload`, {
    method: "POST",
    headers: authHeaders(),
  });
  if (!res.ok) throw new Error(await res.text());
  return res.json() as Promise<{ reloaded: boolean; servers: string[] }>;
}

export async function fetchSessionMessages(sessionId: string) {
  const res = await fetch(`${API}/sessions/${sessionId}/messages`, {
    headers: authHeaders(),
  });
  if (!res.ok) throw new Error(await res.text());
  return res.json() as Promise<{
    messages: Array<{ role: "user" | "assistant"; content: string }>;
  }>;
}

export async function deleteSession(sessionId: string) {
  const res = await fetch(`${API}/sessions/${sessionId}`, {
    method: "DELETE",
    headers: authHeaders(),
  });
  if (!res.ok) throw new Error(await res.text());
}

export async function updateSession(
  sessionId: string,
  body: {
    title?: string;
    model_id?: string;
    project_root?: string;
    permission_mode?: AgentPermissionMode;
  },
) {
  const res = await fetch(`${API}/sessions/${sessionId}`, {
    method: "PATCH",
    headers: authHeaders(),
    body: JSON.stringify(body),
  });
  if (!res.ok) throw new Error(await res.text());
}

export type SkillSummary = {
  name: string;
  description: string;
  file_path: string;
  updated_at?: string | null;
  enabled: boolean;
};

export type SkillUploadResult = {
  name: string;
  description: string;
  file_path: string;
  extracted_files: number;
  skill: SkillSummary;
};

export type ArtifactRecord = {
  id: string;
  session_id: string;
  filename: string;
  mime_type: string;
  size_bytes: number;
  created_at: string;
};

export async function fetchSkills() {
  const res = await fetch(`${API}/skills`, { headers: authHeaders() });
  if (!res.ok) throw new Error(await res.text());
  return res.json() as Promise<SkillSummary[]>;
}

export async function fetchSkill(name: string) {
  const res = await fetch(`${API}/skills/${encodeURIComponent(name)}`, {
    headers: authHeaders(),
  });
  if (!res.ok) throw new Error(await res.text());
  return res.json() as Promise<SkillSummary & { content: string; enabled: boolean }>;
}

export async function updateSkillEnabled(name: string, enabled: boolean) {
  const res = await fetch(`${API}/skills/${encodeURIComponent(name)}`, {
    method: "PATCH",
    headers: authHeaders(),
    body: JSON.stringify({ enabled }),
  });
  if (!res.ok) throw new Error(await res.text());
  return res.json() as Promise<SkillSummary>;
}

export async function uploadSkillZip(file: File, overwrite = false) {
  const form = new FormData();
  form.append("file", file);
  if (overwrite) form.append("overwrite", "true");
  const headers: Record<string, string> = {};
  const token = getApiToken();
  if (token) headers.Authorization = `Bearer ${token}`;
  const res = await fetch(`${API}/skills`, {
    method: "POST",
    headers,
    body: form,
  });
  if (!res.ok) throw new Error(await res.text());
  return res.json() as Promise<SkillUploadResult>;
}

export async function deleteSkill(name: string) {
  const res = await fetch(`${API}/skills/${encodeURIComponent(name)}`, {
    method: "DELETE",
    headers: authHeaders(),
  });
  if (!res.ok) throw new Error(await res.text());
}

export async function listArtifacts(sessionId: string) {
  const res = await fetch(`${API}/sessions/${sessionId}/artifacts`, {
    headers: authHeaders(),
  });
  if (!res.ok) throw new Error(await res.text());
  return res.json() as Promise<ArtifactRecord[]>;
}

export async function fetchArtifactBlob(sessionId: string, artifactId: string): Promise<Blob> {
  const headers: Record<string, string> = {};
  const token = getApiToken();
  if (token) headers.Authorization = `Bearer ${token}`;
  const res = await fetch(`${API}/sessions/${sessionId}/artifacts/${artifactId}`, { headers });
  if (!res.ok) throw new Error(await res.text());
  return res.blob();
}

export async function previewArtifact(
  sessionId: string,
  artifactId: string,
): Promise<{
  id: string;
  filename: string;
  mime_type: string;
  size_bytes: number;
  previewable: boolean;
  kind: "text" | "image" | "binary";
  content: string | null;
  truncated: boolean;
}> {
  const res = await fetch(
    `${API}/sessions/${sessionId}/artifacts/${artifactId}/preview`,
    { headers: authHeaders() },
  );
  if (!res.ok) throw new Error(await res.text());
  return res.json();
}

export async function downloadArtifact(
  sessionId: string,
  artifactId: string,
  filename: string,
) {
  const blob = await fetchArtifactBlob(sessionId, artifactId);
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  a.click();
  URL.revokeObjectURL(url);
}

export type ToolPolicyRecord = {
  id: string;
  tool_pattern: string;
  source_type: string;
  action: string;
  enabled: number;
  created_at: string;
};

export type RunStatus = {
  id: string;
  session_id: string;
  status: string;
  last_seq: number;
  pending_tools: Array<{
    name: string;
    call_id: string;
    args?: Record<string, unknown>;
  }>;
  pending_elicitations: Array<{
    id: string;
    request_type: string;
    message: string;
    url?: string;
  }>;
  error_message: string | null;
};

export async function fetchRun(sessionId: string, runId: string) {
  const res = await fetch(`${API}/sessions/${sessionId}/runs/${runId}`, {
    headers: authHeaders(),
  });
  if (!res.ok) throw new Error(await res.text());
  return res.json() as Promise<RunStatus>;
}

export async function listToolPolicies() {
  const res = await fetch(`${API}/tool-policies`, { headers: authHeaders() });
  if (!res.ok) throw new Error(await res.text());
  return res.json() as Promise<ToolPolicyRecord[]>;
}

export async function createToolPolicy(body: {
  tool_pattern: string;
  source_type: string;
  action: string;
}) {
  const res = await fetch(`${API}/tool-policies`, {
    method: "POST",
    headers: authHeaders(),
    body: JSON.stringify(body),
  });
  if (!res.ok) throw new Error(await res.text());
  return res.json() as Promise<ToolPolicyRecord>;
}

export async function updateToolPolicy(
  id: string,
  body: {
    tool_pattern: string;
    source_type: string;
    action: string;
    enabled?: boolean;
  },
) {
  const res = await fetch(`${API}/tool-policies/${id}`, {
    method: "PATCH",
    headers: authHeaders(),
    body: JSON.stringify(body),
  });
  if (!res.ok) throw new Error(await res.text());
  return res.json() as Promise<ToolPolicyRecord>;
}

export async function deleteToolPolicy(id: string) {
  const res = await fetch(`${API}/tool-policies/${id}`, {
    method: "DELETE",
    headers: authHeaders(),
  });
  if (!res.ok) throw new Error(await res.text());
}

export async function reloadToolPolicies() {
  const res = await fetch(`${API}/tool-policies/reload`, {
    method: "POST",
    headers: authHeaders(),
  });
  if (!res.ok) throw new Error(await res.text());
  return res.json() as Promise<{ reloaded: boolean; enabled_count: number }>;
}

export async function pickProjectDirectory(): Promise<{ cancelled: boolean; path?: string }> {
  const res = await fetch(`${API}/system/pick-directory`, {
    method: "POST",
    headers: authHeaders(),
  });
  if (!res.ok) throw new Error(await res.text());
  return res.json();
}
