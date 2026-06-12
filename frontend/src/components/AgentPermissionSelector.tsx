import { useEffect, useRef, useState } from "react";

export type AgentPermissionMode =
  | "request_approval"
  | "auto_approve"
  | "full_access";

type ModeOption = {
  id: AgentPermissionMode;
  title: string;
  description: string;
  icon: "hand" | "shield-user" | "shield-alert";
};

const MODES: ModeOption[] = [
  {
    id: "request_approval",
    title: "请求批准",
    description: "编辑外部文件和使用互联网时始终询问",
    icon: "hand",
  },
  {
    id: "auto_approve",
    title: "替我审批",
    description: "仅对检测到的风险操作请求批准",
    icon: "shield-user",
  },
  {
    id: "full_access",
    title: "完全访问权限",
    description: "可不受限制地访问互联网和您电脑上的任何文件",
    icon: "shield-alert",
  },
];

function ModeIcon({ kind }: { kind: ModeOption["icon"] }) {
  const common = {
    xmlns: "http://www.w3.org/2000/svg",
    width: 20,
    height: 20,
    viewBox: "0 0 24 24",
    fill: "none",
    stroke: "currentColor",
    strokeWidth: 1.65,
    strokeLinecap: "round" as const,
    strokeLinejoin: "round" as const,
    "aria-hidden": true,
  };

  if (kind === "hand") {
    return (
      <svg {...common}>
        <path d="M7 11V6a2 2 0 1 1 4 0v5" />
        <path d="M11 10V5a2 2 0 1 1 4 0v6" />
        <path d="M15 11V7a2 2 0 1 1 4 0v8a5 5 0 0 1-5 5h-2a5 5 0 0 1-5-5v-3a2 2 0 1 1 4 0" />
      </svg>
    );
  }
  if (kind === "shield-user") {
    return (
      <svg {...common}>
        <path d="M12 3 4 7v6c0 5 3.5 7.5 8 8 4.5-.5 8-3 8-8V7l-8-4z" />
        <circle cx="12" cy="11" r="2.25" />
        <path d="M9.5 15.2a3.5 3.5 0 0 1 5 0" />
      </svg>
    );
  }
  return (
    <svg {...common}>
      <path d="M12 3 4 7v6c0 5 3.5 7.5 8 8 4.5-.5 8-3 8-8V7l-8-4z" />
      <path d="M12 8v5" />
      <circle cx="12" cy="16" r="0.75" fill="currentColor" stroke="none" />
    </svg>
  );
}

type Props = {
  value: AgentPermissionMode;
  disabled?: boolean;
  compact?: boolean;
  menuPlacement?: "above" | "below";
  onChange: (mode: AgentPermissionMode) => void;
};

export function AgentPermissionSelector({
  value,
  disabled,
  compact = false,
  menuPlacement = "below",
  onChange,
}: Props) {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);
  const selected = MODES.find((m) => m.id === value) ?? MODES[0];

  useEffect(() => {
    if (!open) return;
    function onDocClick(e: MouseEvent) {
      if (!rootRef.current?.contains(e.target as Node)) {
        setOpen(false);
      }
    }
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") setOpen(false);
    }
    document.addEventListener("mousedown", onDocClick);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDocClick);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  return (
    <div
      className={`agent-permission-selector${compact ? " agent-permission-selector--compact" : ""}`}
      ref={rootRef}
    >
      <button
        type="button"
        className={`agent-permission-trigger${compact ? " agent-permission-trigger--compact" : ""}`}
        onClick={() => !disabled && setOpen((v) => !v)}
        disabled={disabled}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-label={`权限模式：${selected.title}`}
        title={`${selected.title} — ${selected.description}`}
      >
        <span className="agent-permission-trigger-icon">
          <ModeIcon kind={selected.icon} />
        </span>
        {!compact && (
          <span className="agent-permission-trigger-label">{selected.title}</span>
        )}
      </button>
      {open && (
        <div
          className={`agent-permission-menu${menuPlacement === "above" ? " agent-permission-menu--above" : ""}`}
          role="listbox"
          aria-label="Agent 权限模式"
        >
          {MODES.map((mode) => {
            const active = mode.id === value;
            return (
              <button
                key={mode.id}
                type="button"
                role="option"
                aria-selected={active}
                className={`agent-permission-option ${active ? "active" : ""}`}
                onClick={() => {
                  onChange(mode.id);
                  setOpen(false);
                }}
              >
                <span className="agent-permission-option-icon">
                  <ModeIcon kind={mode.icon} />
                </span>
                <span className="agent-permission-option-text">
                  <span className="agent-permission-option-title">{mode.title}</span>
                  <span className="agent-permission-option-desc">{mode.description}</span>
                </span>
                {active && <span className="agent-permission-check" aria-hidden>✓</span>}
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}
