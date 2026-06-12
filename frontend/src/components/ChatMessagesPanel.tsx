import { useEffect, useRef } from "react";

import { useChatStore } from "../store/chat";
import { ChatMessageItem } from "./ChatMessageItem";

type Props = {
  loading: boolean;
  agentActivity: string | null;
  modelsEmpty: boolean;
  onOpenModels: () => void;
};

function hasAssistantTextAfterLastUser(
  msgs: Array<{ role: string; content: string }>,
): boolean {
  for (let i = msgs.length - 1; i >= 0; i--) {
    if (msgs[i].role === "user") return false;
    if (msgs[i].role === "assistant" && msgs[i].content.trim()) return true;
  }
  return false;
}

export function ChatMessagesPanel({
  loading,
  agentActivity,
  modelsEmpty,
  onOpenModels,
}: Props) {
  const messages = useChatStore((s) => s.messages);
  const chatEndRef = useRef<HTMLDivElement>(null);
  const scrollRafRef = useRef<number | null>(null);

  useEffect(() => {
    if (scrollRafRef.current !== null) {
      cancelAnimationFrame(scrollRafRef.current);
    }
    scrollRafRef.current = requestAnimationFrame(() => {
      scrollRafRef.current = null;
      chatEndRef.current?.scrollIntoView({
        behavior: loading ? "auto" : "smooth",
        block: "end",
      });
    });
  }, [messages, loading]);

  const visible = messages.filter((m) => m.role === "user" || m.content.trim());
  const lastVisibleIndex = visible.length - 1;

  return (
    <div className="chat-scroll">
      {visible.length === 0 ? (
        <div className="chat-empty">
          <h2>个人 Agent</h2>
          <p>在「模型」工具中配置 API 后即可开始对话。</p>
          {modelsEmpty && (
            <button type="button" className="btn btn-primary" onClick={onOpenModels}>
              添加模型
            </button>
          )}
        </div>
      ) : (
        visible.map((m, i) => (
          <ChatMessageItem
            key={`${i}-${m.role}`}
            message={m}
            isStreaming={loading && i === lastVisibleIndex && m.role === "assistant"}
          />
        ))
      )}
      {loading && (
        <div className="chat-activity" aria-live="polite">
          {agentActivity
            || (hasAssistantTextAfterLastUser(messages) ? "Agent 继续处理中…" : "思考中…")}
        </div>
      )}
      <div ref={chatEndRef} />
    </div>
  );
}
