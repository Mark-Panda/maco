import { useEffect, useRef } from "react";
import type { SseEvent } from "../types/sse";

export function useRunStreamState(sessionId: string | null, activeRunId: string | null) {
  const sessionIdRef = useRef<string | null>(sessionId);
  const activeRunIdRef = useRef<string | null>(activeRunId);
  const lastSeqByRunRef = useRef<Record<string, number>>({});

  useEffect(() => {
    sessionIdRef.current = sessionId;
  }, [sessionId]);

  useEffect(() => {
    activeRunIdRef.current = activeRunId;
  }, [activeRunId]);

  function recordEvent(event: SseEvent) {
    if (!event.run_id || typeof event.seq !== "number") return;
    lastSeqByRunRef.current[event.run_id] = Math.max(
      lastSeqByRunRef.current[event.run_id] ?? 0,
      event.seq,
    );
  }

  function afterSeqForRun(runId: string) {
    return lastSeqByRunRef.current[runId];
  }

  function clearRun(runId: string) {
    delete lastSeqByRunRef.current[runId];
  }

  return {
    sessionIdRef,
    activeRunIdRef,
    recordEvent,
    afterSeqForRun,
    clearRun,
  };
}
