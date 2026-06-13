import { useCallback, useEffect, useRef, useState } from "react";
import {
  fetchJobs,
  fetchMemories,
  fetchUsageSummary,
  type JobRecord,
} from "../api/client";

type UsageRow = { key: string; total_tokens: number; estimated_cost: number };
type MemoryRow = { id: number; content: string; timestamp: string };

export function useToolPanelData(sessionId: string | null) {
  const [usage, setUsage] = useState<UsageRow[]>([]);
  const [usageError, setUsageError] = useState<string | null>(null);
  const [memories, setMemories] = useState<MemoryRow[]>([]);
  const [memoryError, setMemoryError] = useState<string | null>(null);
  const [jobs, setJobs] = useState<JobRecord[]>([]);
  const usageRequestRef = useRef(0);
  const memoryRequestRef = useRef(0);
  const jobsRequestRef = useRef(0);

  const refreshUsage = useCallback(async () => {
    const requestId = ++usageRequestRef.current;
    try {
      const rows = await fetchUsageSummary("model");
      if (requestId !== usageRequestRef.current) return;
      setUsage(
        rows.map((r) => ({
          key: r.key,
          total_tokens: r.total_tokens,
          estimated_cost: r.estimated_cost,
        })),
      );
      setUsageError(null);
    } catch (err: unknown) {
      if (requestId !== usageRequestRef.current) return;
      setUsage([]);
      setUsageError(err instanceof Error ? err.message : "加载用量失败");
    }
  }, []);

  const refreshMemories = useCallback(async () => {
    const requestId = ++memoryRequestRef.current;
    try {
      const response = await fetchMemories();
      if (requestId !== memoryRequestRef.current) return;
      setMemories(response.items);
      setMemoryError(null);
    } catch (err: unknown) {
      if (requestId !== memoryRequestRef.current) return;
      setMemories([]);
      setMemoryError(err instanceof Error ? err.message : "加载记忆失败");
    }
  }, []);

  const refreshJobs = useCallback(async () => {
    const requestId = ++jobsRequestRef.current;
    try {
      const rows = await fetchJobs();
      if (requestId !== jobsRequestRef.current) return;
      setJobs(rows);
    } catch {
      if (requestId !== jobsRequestRef.current) return;
      setJobs([]);
    }
  }, []);

  useEffect(() => {
    refreshUsage().catch(() => undefined);
    refreshMemories().catch(() => undefined);
    refreshJobs().catch(() => undefined);
  }, [refreshJobs, refreshMemories, refreshUsage, sessionId]);

  return {
    usage,
    usageError,
    memories,
    memoryError,
    jobs,
    refreshUsage,
    refreshMemories,
    refreshJobs,
  };
}
