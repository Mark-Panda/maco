import { useMemo, useState } from "react";

import type { SessionMeta } from "../api/client";
import { MacoIcon } from "./Icons";

type Props = {
  sessions: SessionMeta[];
  activeSessionId: string | null;
  onSelect: (session: SessionMeta) => void;
  onNewChat: () => void;
  onDelete: (sessionId: string) => void;
  onRename: (sessionId: string, title: string) => Promise<void>;
};

function formatSessionTime(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return "";
  const now = Date.now();
  const diff = now - date.getTime();
  if (diff < 60_000) return "刚刚";
  if (diff < 3_600_000) return `${Math.floor(diff / 60_000)} 分钟前`;
  if (diff < 86_400_000) {
    return date.toLocaleTimeString("zh-CN", { hour: "2-digit", minute: "2-digit" });
  }
  if (diff < 604_800_000) {
    return date.toLocaleDateString("zh-CN", { weekday: "short", hour: "2-digit", minute: "2-digit" });
  }
  return date.toLocaleDateString("zh-CN", { month: "numeric", day: "numeric" });
}

export function ChatSessionSidebar({
  sessions,
  activeSessionId,
  onSelect,
  onNewChat,
  onDelete,
  onRename,
}: Props) {
  const [query, setQuery] = useState("");
  const [editingId, setEditingId] = useState<string | null>(null);
  const [titleDraft, setTitleDraft] = useState("");

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    const sorted = [...sessions].sort(
      (a, b) => new Date(b.updated_at).getTime() - new Date(a.updated_at).getTime(),
    );
    if (!q) return sorted;
    return sorted.filter((s) => {
      const title = (s.title ?? s.session_id.slice(0, 8)).toLowerCase();
      return title.includes(q) || s.session_id.toLowerCase().includes(q);
    });
  }, [query, sessions]);

  async function commitRename(sessionId: string) {
    const next = titleDraft.trim();
    setEditingId(null);
    if (!next) return;
    await onRename(sessionId, next);
  }

  return (
    <aside className="chat-session-sidebar" aria-label="历史会话">
      <div className="chat-session-sidebar-header">
        <h2>历史会话</h2>
        <button type="button" className="btn btn-sm btn-primary chat-session-new" onClick={onNewChat}>
          新对话
        </button>
      </div>

      <div className="chat-session-search">
        <MacoIcon name="sessions" size={16} />
        <input
          type="search"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="搜索会话…"
          aria-label="搜索会话"
        />
      </div>

      <div className="chat-session-list">
        {filtered.length === 0 ? (
          <p className="chat-session-empty">
            {sessions.length === 0 ? "还没有会话，开始新对话吧。" : "没有匹配的会话。"}
          </p>
        ) : (
          filtered.map((s) => {
            const active = s.session_id === activeSessionId;
            const title = s.title ?? `会话 ${s.session_id.slice(0, 8)}`;
            const editing = editingId === s.session_id;

            return (
              <div key={s.session_id} className={`chat-session-item${active ? " active" : ""}`}>
                {editing ? (
                  <input
                    className="chat-session-rename-input"
                    value={titleDraft}
                    autoFocus
                    onChange={(e) => setTitleDraft(e.target.value)}
                    onBlur={() => void commitRename(s.session_id)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") void commitRename(s.session_id);
                      if (e.key === "Escape") setEditingId(null);
                    }}
                  />
                ) : (
                  <button
                    type="button"
                    className="chat-session-item-main"
                    onClick={() => onSelect(s)}
                    onDoubleClick={() => {
                      setEditingId(s.session_id);
                      setTitleDraft(title);
                    }}
                  >
                    <span className="chat-session-item-title">{title}</span>
                    <span className="chat-session-item-meta">
                      {formatSessionTime(s.updated_at)}
                      {s.project_root ? " · 已绑定项目" : ""}
                    </span>
                  </button>
                )}
                <div className="chat-session-item-actions">
                  {!editing ? (
                    <button
                      type="button"
                      className="chat-session-action"
                      title="重命名"
                      aria-label="重命名"
                      onClick={() => {
                        setEditingId(s.session_id);
                        setTitleDraft(title);
                      }}
                    >
                      ✎
                    </button>
                  ) : null}
                  <button
                    type="button"
                    className="chat-session-action chat-session-action--danger"
                    title="删除"
                    aria-label="删除会话"
                    onClick={() => {
                      if (!confirm(`确定删除「${title}」？`)) return;
                      onDelete(s.session_id);
                    }}
                  >
                    ×
                  </button>
                </div>
              </div>
            );
          })
        )}
      </div>
    </aside>
  );
}
