type Todo = { task_key: string; title: string; status: string };

type Artifact = {
  id: string;
  filename: string;
  mime_type: string;
  size_bytes: number;
};

type Props = {
  plan: string;
  todos: Todo[];
  artifacts: Artifact[];
  sessionId: string | null;
  onPreviewArtifact: (artifact: Artifact) => void;
  onPatchTodo: (taskKey: string, status: string) => Promise<void>;
};

export function TasksDock({
  plan,
  todos,
  artifacts,
  sessionId,
  onPreviewArtifact,
  onPatchTodo,
}: Props) {
  return (
    <aside className="tasks-dock" aria-label="任务面板">
      <div className="tasks-dock-header">
        <h2>任务</h2>
        <p>计划、待办与文件产物</p>
      </div>
      <div className="panel-section">
        <h3>文件产物</h3>
        {artifacts.length === 0 ? (
          <p className="panel-empty">Agent 写入的文件会出现在这里，点击可预览。</p>
        ) : (
          artifacts.map((a) => (
            <button
              key={a.id}
              type="button"
              className="artifact-card"
              onClick={() => sessionId && onPreviewArtifact(a)}
              disabled={!sessionId}
            >
              <strong>{a.filename}</strong>
              <div className="model-meta">
                {a.mime_type} · {(a.size_bytes / 1024).toFixed(1)} KB
              </div>
            </button>
          ))
        )}
      </div>
      <div className="panel-section">
        <h3>计划</h3>
        <div className="panel-card">
          <pre className="panel-pre">{plan || "暂无计划"}</pre>
        </div>
      </div>
      <div className="panel-section">
        <h3>待办</h3>
        {todos.length === 0 ? (
          <p className="panel-empty">暂无待办</p>
        ) : (
          todos.map((t) => (
            <div key={t.task_key} className="todo-item">
              <div>{t.title}</div>
              <select
                value={t.status}
                disabled={!sessionId}
                onChange={(e) => onPatchTodo(t.task_key, e.target.value)}
              >
                <option value="pending">待处理</option>
                <option value="in_progress">进行中</option>
                <option value="completed">已完成</option>
              </select>
            </div>
          ))
        )}
      </div>
    </aside>
  );
}
