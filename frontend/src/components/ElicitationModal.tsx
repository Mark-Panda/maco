type Props = {
  message: string;
  requestType: string;
  url?: string;
  input: string;
  submitting: boolean;
  onInputChange: (value: string) => void;
  onAccept: () => void;
  onDecline: () => void;
};

export function ElicitationModal({
  message,
  requestType,
  url,
  input,
  submitting,
  onInputChange,
  onAccept,
  onDecline,
}: Props) {
  return (
    <div className="run-modal-overlay" role="presentation">
      <div
        className="run-modal run-modal-info"
        role="dialog"
        aria-modal="true"
        aria-labelledby="elicitation-modal-title"
        onClick={(e) => e.stopPropagation()}
      >
        <header className="run-modal-header">
          <h2 id="elicitation-modal-title">需要 MCP 输入</h2>
          <p>{message}</p>
        </header>
        <div className="run-modal-body">
          {url && (
            <a href={url} target="_blank" rel="noreferrer" className="run-modal-link">
              打开链接
            </a>
          )}
          {requestType === "form" && (
            <textarea
              className="chat-input"
              style={{ width: "100%", minHeight: 96, marginTop: url ? 12 : 0 }}
              value={input}
              onChange={(e) => onInputChange(e.target.value)}
              placeholder='JSON，例如 {"key": "value"}'
            />
          )}
        </div>
        <footer className="run-modal-footer">
          <button type="button" className="btn btn-sm" onClick={onDecline} disabled={submitting}>
            拒绝
          </button>
          <button type="button" className="btn btn-sm btn-primary" onClick={onAccept} disabled={submitting}>
            提交
          </button>
        </footer>
      </div>
    </div>
  );
}
