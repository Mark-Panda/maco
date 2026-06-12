import { memo } from "react";

import type { Message } from "../store/chat";
import { ChatMessageContent } from "./ChatMessageContent";

type Props = {
  message: Message;
  isStreaming: boolean;
};

export const ChatMessageItem = memo(function ChatMessageItem({ message, isStreaming }: Props) {
  return (
    <div className={`msg msg-${message.role}`}>
      <span className="msg-label">{message.role === "user" ? "你" : "助手"}</span>
      <div className="msg-bubble">
        <ChatMessageContent role={message.role} content={message.content} streaming={isStreaming} />
      </div>
    </div>
  );
});
