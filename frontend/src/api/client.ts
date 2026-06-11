const API = "/api";
const TOKEN_KEY = "maco_api_token";

export type ModelView = {
  id: string;
  name: string;
  provider: "openai" | "anthropic";
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

export async function createSession(title?: string, modelId?: string) {
  const res = await fetch(`${API}/sessions`, {
    method: "POST",
    headers: authHeaders(),
    body: JSON.stringify({ title: title ?? "New chat", model_id: modelId }),
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
) {
  const res = await fetch(`${API}/chat`, {
    method: "POST",
    headers: authHeaders(),
    body: JSON.stringify({
      session_id: sessionId,
      message,
      model_id: modelId,
    }),
  });
  await consumeSse(res, onEvent);
}

export async function resumeRun(
  sessionId: string,
  runId: string,
  approved: boolean,
  onEvent: (data: Record<string, unknown>) => void,
  note?: string,
) {
  const res = await fetch(`${API}/sessions/${sessionId}/runs/${runId}/resume`, {
    method: "POST",
    headers: authHeaders(),
    body: JSON.stringify({ approved, note }),
  });
  await consumeSse(res, onEvent);
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
