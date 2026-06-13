import { useEffect, useState } from "react";

import type { SubAgentActivity } from "../types/subAgentActivity";
import { MacoIcon } from "./Icons";

type Props = {
  activity: SubAgentActivity | undefined;
  inProgress: boolean;
};

function formatElapsed(startedAt: number, now: number): string {
  const sec = Math.max(0, Math.floor((now - startedAt) / 1000));
  if (sec < 60) return `${sec}s`;
  const min = Math.floor(sec / 60);
  return `${min}m ${sec % 60}s`;
}

export function SubAgentLane({ activity, inProgress }: Props) {
  const [now, setNow] = useState(() => Date.now());

  useEffect(() => {
    if (!inProgress && !activity) return;
    const id = window.setInterval(() => setNow(Date.now()), 5000);
    return () => window.clearInterval(id);
  }, [inProgress, activity]);

  if (!inProgress) return null;

  const worker = activity?.worker_agent ?? "worker";
  const hasOutput = Boolean(activity?.last_progress?.trim());

  return (
    <div
      className={`todo-sub-agent-lane${activity ? " todo-sub-agent-lane--active" : ""}`}
      aria-label="子 Agent 执行泳道"
    >
      <div className="todo-sub-agent-lane-header">
        <span className="todo-sub-agent-lane-icon" aria-hidden>
          <MacoIcon name="gitBranch" size={14} />
        </span>
        <span className="todo-sub-agent-lane-worker">{worker}</span>
        {activity?.started_at ? (
          <span className="todo-sub-agent-lane-elapsed">
            {formatElapsed(activity.started_at, now)}
          </span>
        ) : null}
      </div>
      {activity?.last_tool ? (
        <p className="todo-sub-agent-lane-meta">工具：{activity.last_tool}</p>
      ) : !activity ? (
        <p className="todo-sub-agent-lane-meta todo-sub-agent-lane-meta--muted">
          等待 spawn_sub_agent…
        </p>
      ) : null}
      {hasOutput ? (
        <details className="todo-sub-agent-lane-output" open>
          <summary>子 Agent 输出</summary>
          <p className="todo-sub-agent-lane-snippet">{activity?.last_progress}</p>
        </details>
      ) : activity?.last_tool ? (
        <p className="todo-sub-agent-lane-snippet todo-sub-agent-lane-snippet--muted">
          执行中…
        </p>
      ) : null}
    </div>
  );
}
