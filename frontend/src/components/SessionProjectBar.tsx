type Props = {
  projectRootDraft: string;
  pickingFolder: boolean;
  hasSession: boolean;
  onPickFolder: () => void;
  onClear: () => void;
};

export function SessionProjectBar({
  projectRootDraft,
  pickingFolder,
  hasSession,
  onPickFolder,
  onClear,
}: Props) {
  return (
    <div className="session-project-bar">
      <span className="session-project-bar-label">项目路径</span>
      {projectRootDraft ? (
        <span className="project-path-display session-project-bar-path" title={projectRootDraft}>
          {projectRootDraft}
        </span>
      ) : (
        <span className="session-project-bar-empty">未绑定项目文件夹</span>
      )}
      <div className="session-project-bar-actions">
        <button
          type="button"
          className="btn btn-sm btn-primary"
          onClick={onPickFolder}
          disabled={pickingFolder}
        >
          {pickingFolder ? "选择中…" : "选择文件夹"}
        </button>
        {projectRootDraft && (
          <button type="button" className="btn btn-sm" onClick={onClear}>
            清除
          </button>
        )}
      </div>
      {!hasSession && (
        <span className="session-project-bar-hint">发首条消息前选择，将随新会话保存</span>
      )}
    </div>
  );
}
