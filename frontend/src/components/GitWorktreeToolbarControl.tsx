import { useEffect, useRef, useState } from "react";
import {
  DEFAULT_GIT_BRANCH_PREFIX,
  type GitWorktreeStatus,
} from "../api/client";
import { MacoIcon } from "./Icons";

const STATUS_HINTS: Record<
  Exclude<GitWorktreeStatus, "disabled" | "active">,
  { tone: "info" | "warn"; text: string }
> = {
  no_project: {
    tone: "info",
    text: "尚未绑定项目文件夹，worktree 将在绑定 Git 项目后生效。",
  },
  not_git_repo: {
    tone: "warn",
    text: "当前项目不是 Git 仓库，无法创建 worktree，Agent 将在项目目录直接编辑。",
  },
  git_unavailable: {
    tone: "warn",
    text: "未检测到 git 命令，请安装 Git 并确保其在 PATH 中。",
  },
  pending: {
    tone: "info",
    text: "worktree 尚未创建，发送首条消息或切换项目后将自动 provision。",
  },
};

type Props = {
  enabled: boolean;
  branchPrefix: string;
  worktreePath: string | null;
  worktreeBranch: string | null;
  worktreeStatus: GitWorktreeStatus;
  disabled?: boolean;
  onEnabledChange: (enabled: boolean) => void;
  onBranchPrefixChange: (prefix: string) => void;
  onBranchPrefixCommit: () => void;
};

export function GitWorktreeToolbarControl({
  enabled,
  branchPrefix,
  worktreePath,
  worktreeBranch,
  worktreeStatus,
  disabled,
  onEnabledChange,
  onBranchPrefixChange,
  onBranchPrefixCommit,
}: Props) {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);
  const hint =
    enabled && worktreeStatus !== "active" && worktreeStatus !== "disabled"
      ? STATUS_HINTS[worktreeStatus]
      : null;
  const showWarnDot =
    enabled &&
    (worktreeStatus === "not_git_repo" || worktreeStatus === "git_unavailable");
  const showActiveDot = enabled && worktreeStatus === "active";

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
    <div className="git-worktree-toolbar" ref={rootRef}>
      <button
        type="button"
        className={`toolbar-tab git-worktree-toolbar-trigger${open ? " active" : ""}${enabled ? " git-worktree-toolbar-trigger--on" : ""}`}
        onClick={() => !disabled && setOpen((v) => !v)}
        disabled={disabled}
        title="Git worktree 全局设置"
        aria-haspopup="dialog"
        aria-expanded={open}
      >
        <span className="toolbar-tab-icon">
          <MacoIcon name="gitBranch" size={18} />
          {showActiveDot && <span className="toolbar-run-dot active" aria-hidden />}
          {showWarnDot && <span className="toolbar-run-dot warn" aria-hidden />}
        </span>
        <span className="toolbar-tab-label">Worktree</span>
      </button>
      {open && (
        <div className="git-worktree-toolbar-panel" role="dialog" aria-label="Git worktree 设置">
          <h4 className="git-worktree-toolbar-title">Git worktree</h4>
          <p className="git-worktree-toolbar-desc">
            全局默认：绑定 Git 项目后，Agent 在独立 worktree 中编辑，避免污染主仓库。
          </p>
          {hint && (
            <p
              className={`git-worktree-toolbar-alert git-worktree-toolbar-alert--${hint.tone}`}
              role="status"
            >
              {hint.text}
            </p>
          )}
          <label className="git-worktree-toolbar-toggle">
            <input
              type="checkbox"
              checked={enabled}
              disabled={disabled}
              onChange={(e) => onEnabledChange(e.target.checked)}
            />
            <span>强制 Git worktree 编辑</span>
          </label>
          <label className="git-worktree-toolbar-prefix">
            <span>分支前缀</span>
            <input
              type="text"
              value={branchPrefix}
              disabled={disabled || !enabled}
              placeholder={DEFAULT_GIT_BRANCH_PREFIX}
              onChange={(e) => onBranchPrefixChange(e.target.value)}
              onBlur={() => onBranchPrefixCommit()}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  onBranchPrefixCommit();
                }
              }}
            />
          </label>
          {worktreePath && enabled && worktreeStatus === "active" && (
            <p className="git-worktree-toolbar-meta" title={worktreePath}>
              当前会话：{worktreePath}
              {worktreeBranch ? ` · ${worktreeBranch}` : ""}
            </p>
          )}
        </div>
      )}
    </div>
  );
}
