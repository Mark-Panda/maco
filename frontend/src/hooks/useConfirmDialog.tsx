import { useCallback, useRef, useState, type ReactNode } from "react";

import { ConfirmDialog } from "../components/ConfirmDialog";

export type ConfirmRequest = {
  title: string;
  description: string;
  confirmLabel?: string;
  cancelLabel?: string;
  tone?: "danger" | "warn" | "default";
};

export function useConfirmDialog() {
  const [pending, setPending] = useState<ConfirmRequest | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const actionRef = useRef<(() => void | Promise<void>) | null>(null);

  const runConfirm = useCallback((options: ConfirmRequest, action: () => void | Promise<void>) => {
    actionRef.current = action;
    setPending(options);
  }, []);

  const dialog: ReactNode = pending ? (
    <ConfirmDialog
      title={pending.title}
      description={pending.description}
      confirmLabel={pending.confirmLabel}
      cancelLabel={pending.cancelLabel}
      tone={pending.tone}
      submitting={submitting}
      onCancel={() => {
        if (!submitting) {
          setPending(null);
          actionRef.current = null;
        }
      }}
      onConfirm={() => {
        if (submitting || !actionRef.current) return;
        const action = actionRef.current;
        setSubmitting(true);
        void Promise.resolve(action())
          .then(() => {
            setPending(null);
            actionRef.current = null;
          })
          .finally(() => setSubmitting(false));
      }}
    />
  ) : null;

  return { runConfirm, dialog };
}
