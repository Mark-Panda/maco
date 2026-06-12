import { useEffect } from "react";

type Props = {
  title: string;
  description: string;
  confirmLabel?: string;
  cancelLabel?: string;
  tone?: "danger" | "warn" | "default";
  submitting?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
};

export function ConfirmDialog({
  title,
  description,
  confirmLabel = "确定",
  cancelLabel = "取消",
  tone = "default",
  submitting = false,
  onConfirm,
  onCancel,
}: Props) {
  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape" && !submitting) {
        onCancel();
      }
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onCancel, submitting]);

  const toneClass =
    tone === "danger" ? "run-modal-danger" : tone === "warn" ? "run-modal-warn" : "";

  return (
    <div
      className="run-modal-overlay"
      role="presentation"
      onClick={() => {
        if (!submitting) onCancel();
      }}
    >
      <div
        className={`run-modal ${toneClass}`.trim()}
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="confirm-dialog-title"
        aria-describedby="confirm-dialog-desc"
        onClick={(e) => e.stopPropagation()}
      >
        <header className="run-modal-header">
          <h2 id="confirm-dialog-title">{title}</h2>
          <p id="confirm-dialog-desc">{description}</p>
        </header>
        <footer className="run-modal-footer">
          <button type="button" className="btn btn-sm" onClick={onCancel} disabled={submitting}>
            {cancelLabel}
          </button>
          <button
            type="button"
            className={`btn btn-sm ${tone === "danger" ? "btn-danger" : "btn-primary"}`}
            onClick={onConfirm}
            disabled={submitting}
          >
            {confirmLabel}
          </button>
        </footer>
      </div>
    </div>
  );
}
