import { useEffect, useState } from "react";
import ReactMarkdown from "react-markdown";

import { downloadArtifact } from "../api/client";
import { MacoIcon, type MacoIconName } from "./Icons";
import { ArtifactPreviewContent } from "./ArtifactPreviewContent";
import { SubAgentLane } from "./SubAgentLane";
import { MARKDOWN_COMPONENTS, MARKDOWN_REMARK_PLUGINS } from "./markdownComponents";
import { TasksDockSkeleton } from "./TasksDockSkeleton";
import { normalizeAssistantMarkdown } from "../utils/normalizeMarkdown";
import { extractPlanTitle, stripPlanChecklist } from "../utils/planMarkdown";
import type { SubAgentActivityMap } from "../types/subAgentActivity";

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
  busy?: boolean;
  subAgentActivity?: SubAgentActivityMap;
};

type TodoTone = "pending" | "in_progress" | "completed";

function todoStatusMeta(status: string): { label: string; tone: TodoTone } {
  const normalized = status.trim().toLowerCase();
  if (normalized === "completed" || normalized === "done" || normalized === "complete") {
    return { label: "已完成", tone: "completed" };
  }
  if (normalized === "in_progress" || normalized === "in-progress" || normalized === "doing") {
    return { label: "进行中", tone: "in_progress" };
  }
  return { label: "待处理", tone: "pending" };
}

function formatArtifactSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function artifactExt(filename: string): string {
  const dot = filename.lastIndexOf(".");
  if (dot <= 0 || dot === filename.length - 1) return "FILE";
  return filename.slice(dot + 1).toUpperCase().slice(0, 4);
}

function progressHeadline(completed: number, total: number): string {
  if (total === 0) return "等待任务开始";
  if (completed >= total) return "全部完成";
  if (completed === 0) return "准备执行";
  return "执行中";
}

function PanelEmpty({ icon, children }: { icon: MacoIconName; children: string }) {
  return (
    <div className="panel-empty panel-empty--illustrated">
      <span className="panel-empty-icon" aria-hidden>
        <MacoIcon name={icon} size={18} />
      </span>
      <p>{children}</p>
    </div>
  );
}

export function TasksDock({
  plan,
  todos,
  artifacts,
  sessionId,
  busy = false,
  subAgentActivity = {},
}: Props) {
  const [planOpen, setPlanOpen] = useState(false);
  const [previewArtifactId, setPreviewArtifactId] = useState<string | null>(null);

  const completedCount = todos.filter((t) => todoStatusMeta(t.status).tone === "completed").length;
  const inProgressCount = todos.filter((t) => todoStatusMeta(t.status).tone === "in_progress").length;
  const progressPct = todos.length ? Math.round((completedCount / todos.length) * 100) : 0;
  const allDone = todos.length > 0 && completedCount >= todos.length;

  const planMarkdown = plan ? normalizeAssistantMarkdown(plan) : "";
  const planTitle = plan ? extractPlanTitle(planMarkdown) : null;
  const planNarrative = todos.length > 0 ? stripPlanChecklist(planMarkdown) : planMarkdown;
  const hasPlanNarrative = planNarrative.length > 0;
  const showUnifiedTaskSection = todos.length > 0 || !!plan;
  const showTaskSkeleton = busy && todos.length === 0 && !plan;

  const previewArtifact = artifacts.find((a) => a.id === previewArtifactId) ?? null;

  useEffect(() => {
    if (allDone) setPlanOpen(false);
  }, [allDone]);

  useEffect(() => {
    if (previewArtifactId && !artifacts.some((a) => a.id === previewArtifactId)) {
      setPreviewArtifactId(null);
    }
  }, [artifacts, previewArtifactId]);

  const planBlockVisible =
    !!plan && (todos.length === 0 ? hasPlanNarrative : planOpen && hasPlanNarrative);

  const heroClass = [
    "tasks-dock-hero",
    allDone ? "tasks-dock-hero--done" : "",
    inProgressCount > 0 || (busy && !allDone) ? "tasks-dock-hero--active" : "",
    busy && todos.length === 0 ? "tasks-dock-hero--loading" : "",
  ]
    .filter(Boolean)
    .join(" ");

  return (
    <aside className="tasks-dock" aria-label="任务面板">
      <div className={heroClass}>
        <div className="tasks-dock-hero-copy">
          <div className="tasks-dock-hero-icon" aria-hidden>
            <MacoIcon name="tasks" size={18} />
          </div>
          <div className="tasks-dock-hero-text">
            <h2>{progressHeadline(completedCount, todos.length)}</h2>
            <p>
              {busy && todos.length === 0
                ? "Agent 正在规划与执行…"
                : todos.length > 0
                  ? `已完成 ${completedCount} / ${todos.length} 项`
                  : "计划、待办与文件产物"}
            </p>
            {planTitle ? <p className="tasks-dock-hero-plan">{planTitle}</p> : null}
          </div>
        </div>
        {todos.length > 0 ? (
          <div
            className={`tasks-progress-ring${allDone ? " tasks-progress-ring--done" : ""}`}
            style={{ ["--progress" as string]: `${progressPct}` }}
            aria-label={`进度 ${progressPct}%`}
          >
            <span className="tasks-progress-ring-inner">
              {allDone ? "✓" : `${progressPct}%`}
            </span>
          </div>
        ) : busy ? (
          <div className="tasks-progress-ring tasks-progress-ring--loading" aria-hidden>
            <span className="tasks-progress-ring-inner">…</span>
          </div>
        ) : null}
      </div>

      {showUnifiedTaskSection ? (
        <section className="tasks-dock-section tasks-unified">
          <div className="tasks-section-row">
            <h3 className="tasks-section-label">任务与计划</h3>
            {todos.length > 0 && hasPlanNarrative ? (
              <button
                type="button"
                className="plan-inline-toggle"
                aria-expanded={planOpen}
                onClick={() => setPlanOpen((open) => !open)}
              >
                {planOpen ? "收起说明" : "背景说明"}
              </button>
            ) : null}
          </div>

          {showTaskSkeleton ? (
            <TasksDockSkeleton />
          ) : (
            <div className="tasks-unified-card">
              {todos.length > 0 ? (
                <ol className="todo-timeline">
                  {todos.map((t, index) => {
                    const meta = todoStatusMeta(t.status);
                    const isLast = index === todos.length - 1 && !planBlockVisible;
                    return (
                      <li
                        key={t.task_key}
                        className={`todo-step todo-step--${meta.tone}${isLast ? " todo-step--last" : ""}`}
                      >
                        <div className="todo-step-rail" aria-hidden>
                          <span className="todo-step-node">{index + 1}</span>
                        </div>
                        <div className="todo-step-body">
                          <span className="todo-step-title">{t.title}</span>
                          <span className={`todo-step-status todo-step-status--${meta.tone}`}>
                            {meta.label}
                          </span>
                          <SubAgentLane
                            inProgress={meta.tone === "in_progress"}
                            activity={subAgentActivity[t.task_key]}
                          />
                        </div>
                      </li>
                    );
                  })}
                </ol>
              ) : null}

              {todos.length > 0 && hasPlanNarrative ? (
                <div
                  className={`plan-panel-wrap${planBlockVisible ? " plan-panel-wrap--open" : ""}`}
                >
                  <div className="plan-panel panel-card plan-panel--nested">
                    <div className="plan-panel-body markdown-body">
                      <ReactMarkdown remarkPlugins={MARKDOWN_REMARK_PLUGINS} components={MARKDOWN_COMPONENTS}>
                        {planNarrative}
                      </ReactMarkdown>
                    </div>
                  </div>
                </div>
              ) : null}

              {planBlockVisible && todos.length === 0 ? (
                <div className="plan-panel panel-card">
                  <div className="plan-panel-body markdown-body">
                    <ReactMarkdown remarkPlugins={MARKDOWN_REMARK_PLUGINS} components={MARKDOWN_COMPONENTS}>
                      {planMarkdown}
                    </ReactMarkdown>
                  </div>
                </div>
              ) : null}

              {!plan && todos.length === 0 && !busy ? (
                <PanelEmpty icon="tasks">Agent 开始多步任务后会在这里展示进度。</PanelEmpty>
              ) : null}
            </div>
          )}
        </section>
      ) : (
        <section className="tasks-dock-section">
          <h3 className="tasks-section-label">任务与计划</h3>
          {showTaskSkeleton ? (
            <TasksDockSkeleton />
          ) : (
            <PanelEmpty icon="tasks">Agent 开始多步任务后会在这里展示进度。</PanelEmpty>
          )}
        </section>
      )}

      <section className="tasks-dock-section">
        <div className="tasks-section-row">
          <h3 className="tasks-section-label">文件产物</h3>
          {artifacts.length > 0 ? <span className="panel-count">{artifacts.length}</span> : null}
        </div>
        {artifacts.length === 0 ? (
          <PanelEmpty icon="paperclip">Agent 写入的文件会出现在这里，点击可预览。</PanelEmpty>
        ) : (
          <div className="artifact-list">
            {artifacts.map((a) => {
              const selected = previewArtifactId === a.id;
              return (
                <button
                  key={a.id}
                  type="button"
                  className={`artifact-card${selected ? " artifact-card--active" : ""}`}
                  aria-expanded={selected}
                  onClick={() => {
                    if (!sessionId) return;
                    setPreviewArtifactId((id) => (id === a.id ? null : a.id));
                  }}
                  disabled={!sessionId}
                >
                  <span className="artifact-ext" aria-hidden>
                    {artifactExt(a.filename)}
                  </span>
                  <span className="artifact-body">
                    <strong>{a.filename}</strong>
                    <span className="model-meta">
                      {a.mime_type} · {formatArtifactSize(a.size_bytes)}
                    </span>
                  </span>
                  <span className="artifact-action" aria-hidden>
                    {selected ? "收起" : "预览"}
                  </span>
                </button>
              );
            })}
          </div>
        )}

        {previewArtifact && sessionId ? (
          <div className="artifact-inline-preview panel-card">
            <div className="artifact-inline-preview-header">
              <div>
                <strong>{previewArtifact.filename}</strong>
                <span className="model-meta">
                  {previewArtifact.mime_type} · {formatArtifactSize(previewArtifact.size_bytes)}
                </span>
              </div>
              <div className="artifact-inline-preview-actions">
                <button
                  type="button"
                  className="btn btn-sm"
                  onClick={() =>
                    downloadArtifact(sessionId, previewArtifact.id, previewArtifact.filename).catch(
                      () => undefined,
                    )
                  }
                >
                  下载
                </button>
                <button
                  type="button"
                  className="btn btn-sm btn-ghost"
                  onClick={() => setPreviewArtifactId(null)}
                >
                  关闭
                </button>
              </div>
            </div>
            <div className="artifact-inline-preview-body">
              <ArtifactPreviewContent sessionId={sessionId} artifact={previewArtifact} />
            </div>
          </div>
        ) : null}
      </section>
    </aside>
  );
}
