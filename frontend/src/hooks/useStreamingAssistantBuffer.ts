import { useCallback, useRef } from "react";

import { useChatStore } from "../store/chat";

/** 将 SSE 文本 chunk 合并到 rAF 帧内批量写入，减少重渲染次数。 */
export function useStreamingAssistantBuffer() {
  const appendAssistant = useChatStore((s) => s.appendAssistant);
  const bufferRef = useRef("");
  const rafRef = useRef<number | null>(null);

  const flush = useCallback(() => {
    if (rafRef.current !== null) {
      cancelAnimationFrame(rafRef.current);
      rafRef.current = null;
    }
    if (bufferRef.current) {
      appendAssistant(bufferRef.current);
      bufferRef.current = "";
    }
  }, [appendAssistant]);

  const appendChunk = useCallback(
    (chunk: string) => {
      if (!chunk) return;
      bufferRef.current += chunk;
      if (rafRef.current !== null) return;
      rafRef.current = requestAnimationFrame(() => {
        rafRef.current = null;
        if (bufferRef.current) {
          appendAssistant(bufferRef.current);
          bufferRef.current = "";
        }
      });
    },
    [appendAssistant],
  );

  return { appendChunk, flush };
}
