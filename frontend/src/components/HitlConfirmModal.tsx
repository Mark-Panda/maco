type Props = {
  toolName: string;
  toolArgs?: Record<string, unknown> | null;
  submitting: boolean;
  onApprove: () => void;
  onReject: () => void;
};

function formatToolPreview(toolName: string, args: Record<string, unknown> | null | undefined): string {
  if (!args) return toolName;
  if (toolName === "bash" && typeof args.command === "string") {
    return args.command;
  }
  try {
    return JSON.stringify(args, null, 2);
  } catch {
    return toolName;
  }
}

export function HitlConfirmModal({
  toolName,
  toolArgs,
  submitting,
  onApprove,
  onReject,
}: Props) {
  const preview = formatToolPreview(toolName, toolArgs);
  const showCommand = preview !== toolName;

  return (
    <div className="run-modal-overlay" role="presentation">
      <div
        className="run-modal run-modal-warn"
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="hitl-modal-title"
        onClick={(e) => e.stopPropagation()}
      >
        <header className="run-modal-header">
          <h2 id="hitl-modal-title">工具调用确认</h2>
          <p>Agent 请求执行以下工具，请批准或拒绝。</p>
        </header>
        <div className="run-modal-body">
          <div className="run-modal-tool-label">工具</div>
          <code className="run-modal-tool-name">{toolName}</code>
          {showCommand && (
            <>
              <div className="run-modal-tool-label">命令 / 参数</div>
              <pre className="run-modal-command">{preview}</pre>
            </>
          )}
        </div>
        <footer className="run-modal-footer">
          <button type="button" className="btn btn-sm" onClick={onReject} disabled={submitting}>
            拒绝
          </button>
          <button type="button" className="btn btn-sm btn-primary" onClick={onApprove} disabled={submitting}>
            批准
          </button>
        </footer>
      </div>
    </div>
  );
}
