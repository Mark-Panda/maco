import { memo } from "react";
import ReactMarkdown from "react-markdown";

import { normalizeAssistantMarkdown } from "../utils/normalizeMarkdown";
import { MARKDOWN_COMPONENTS, MARKDOWN_REMARK_PLUGINS } from "./markdownComponents";

type Props = {
  role: "user" | "assistant";
  content: string;
  /** 流式输出中：纯文本展示，避免每帧全量 Markdown 解析。 */
  streaming?: boolean;
};

export const ChatMessageContent = memo(function ChatMessageContent({
  role,
  content,
  streaming = false,
}: Props) {
  if (role === "user") {
    return <>{content}</>;
  }

  if (streaming) {
    return <div className="markdown-streaming">{content}</div>;
  }

  const markdown = normalizeAssistantMarkdown(content);

  return (
    <div className="markdown-body">
      <ReactMarkdown remarkPlugins={MARKDOWN_REMARK_PLUGINS} components={MARKDOWN_COMPONENTS}>
        {markdown}
      </ReactMarkdown>
    </div>
  );
});
