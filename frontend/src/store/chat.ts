import { create } from "zustand";

export type Message = { role: "user" | "assistant"; content: string };

type ChatState = {
  sessionId: string | null;
  messages: Message[];
  /** 下一次 `appendAssistant` 时开启新气泡（工具调用后的新一轮模型输出）。 */
  pendingAssistantTurn: boolean;
  setSessionId: (id: string | null) => void;
  setMessages: (msgs: Message[]) => void;
  pushMessage: (msg: Message) => void;
  /** 工具调用前丢弃当前助手气泡（中间推理/英文规划不展示）。 */
  dropAssistantTurnBeforeTool: () => void;
  appendAssistant: (chunk: string) => void;
  reset: () => void;
  clearMessages: () => void;
};

export const useChatStore = create<ChatState>((set) => ({
  sessionId: null,
  messages: [],
  pendingAssistantTurn: false,
  setSessionId: (id) => set({ sessionId: id }),
  setMessages: (msgs) => set({ messages: msgs, pendingAssistantTurn: false }),
  pushMessage: (msg) =>
    set((s) => ({
      messages: [...s.messages, msg],
      pendingAssistantTurn: msg.role === "user" ? false : s.pendingAssistantTurn,
    })),
  dropAssistantTurnBeforeTool: () =>
    set((s) => {
      const msgs = [...s.messages];
      if (msgs.length > 0 && msgs[msgs.length - 1]?.role === "assistant") {
        msgs.pop();
      }
      return { messages: msgs, pendingAssistantTurn: true };
    }),
  appendAssistant: (chunk) =>
    set((s) => {
      if (!chunk) return s;
      if (s.pendingAssistantTurn) {
        return {
          pendingAssistantTurn: false,
          messages: [...s.messages, { role: "assistant", content: chunk }],
        };
      }
      const msgs = [...s.messages];
      const last = msgs[msgs.length - 1];
      if (last?.role === "assistant") {
        msgs[msgs.length - 1] = { ...last, content: last.content + chunk };
      } else {
        msgs.push({ role: "assistant", content: chunk });
      }
      return { messages: msgs };
    }),
  reset: () => set({ messages: [], sessionId: null, pendingAssistantTurn: false }),
  clearMessages: () => set({ messages: [], pendingAssistantTurn: false }),
}));
