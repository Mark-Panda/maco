export function TasksDockSkeleton() {
  return (
    <div className="tasks-dock-skeleton" aria-hidden>
      <div className="tasks-unified-card">
        {[0, 1, 2].map((i) => (
          <div key={i} className="tasks-dock-skeleton-step">
            <div className="tasks-dock-skeleton-node" />
            <div className="tasks-dock-skeleton-body">
              <div className="tasks-dock-skeleton-line tasks-dock-skeleton-line--title" />
              <div className="tasks-dock-skeleton-line tasks-dock-skeleton-line--short" />
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
