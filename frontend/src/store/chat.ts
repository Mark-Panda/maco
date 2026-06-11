import { create } from "zustand";

export type Message = { role: "user" | "assistant"; content: string };

type ChatState = {
  sessionId: string | null;
  messages: Message[];
  setSessionId: (id: string | null) => void;
  setMessages: (msgs: Message[]) => void;
  pushMessage: (msg: Message) => void;
  appendAssistant: (chunk: string) => void;
  reset: () => void;
  clearMessages: () => void;
};

export const useChatStore = create<ChatState>((set) => ({
  sessionId: null,
  messages: [],
  setSessionId: (id) => set({ sessionId: id }),
  setMessages: (msgs) => set({ messages: msgs }),
  pushMessage: (msg) => set((s) => ({ messages: [...s.messages, msg] })),
  appendAssistant: (chunk) =>
    set((s) => {
      const msgs = [...s.messages];
      const last = msgs[msgs.length - 1];
      if (last?.role === "assistant") {
        msgs[msgs.length - 1] = { ...last, content: last.content + chunk };
      } else {
        msgs.push({ role: "assistant", content: chunk });
      }
      return { messages: msgs };
    }),
  reset: () => set({ messages: [], sessionId: null }),
  clearMessages: () => set({ messages: [] }),
}));
