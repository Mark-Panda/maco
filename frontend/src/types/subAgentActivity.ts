/** 任务看板中单个 todo 的子 Agent 实时状态（由 SSE 驱动）。 */
export type SubAgentActivity = {
  task_key: string;
  worker_agent?: string;
  started_at: number;
  last_progress?: string;
  last_tool?: string;
};

export type SubAgentActivityMap = Record<string, SubAgentActivity>;

export function upsertSubAgentActivity(
  prev: SubAgentActivityMap,
  taskKey: string,
  patch: Partial<Omit<SubAgentActivity, "task_key">>,
): SubAgentActivityMap {
  const existing = prev[taskKey];
  return {
    ...prev,
    [taskKey]: {
      task_key: taskKey,
      started_at: existing?.started_at ?? Date.now(),
      worker_agent: existing?.worker_agent,
      last_progress: existing?.last_progress,
      last_tool: existing?.last_tool,
      ...patch,
    },
  };
}

export function pruneCompletedSubAgentActivity(
  prev: SubAgentActivityMap,
  todos: Array<{ task_key: string; status: string }>,
): SubAgentActivityMap {
  const next = { ...prev };
  for (const t of todos) {
    const s = t.status.trim().toLowerCase();
    if (s === "completed" || s === "done" || s === "complete") {
      delete next[t.task_key];
    }
  }
  return next;
}
