type Props = {
  message: string;
  onGoToChat: () => void;
  onStop: () => void;
  stopping?: boolean;
};

export function RunStatusBar({ message, onGoToChat, onStop, stopping }: Props) {
  return (
    <div className="run-status-bar" role="status">
      <span className="run-status-bar-dot" aria-hidden />
      <span className="run-status-bar-text">{message}</span>
      <div className="run-status-bar-actions">
        <button type="button" className="btn btn-sm btn-primary" onClick={onGoToChat}>
          回到会话
        </button>
        <button type="button" className="btn btn-sm" onClick={onStop} disabled={stopping}>
          停止
        </button>
      </div>
    </div>
  );
}
