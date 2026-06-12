import { useCallback, useEffect, useState } from "react";

import {
  getTasksDockWidth,
  persistTasksDockWidth,
  TASKS_DOCK_WIDTH_MAX,
  TASKS_DOCK_WIDTH_MIN,
} from "../api/client";

function clampWidth(width: number): number {
  return Math.min(TASKS_DOCK_WIDTH_MAX, Math.max(TASKS_DOCK_WIDTH_MIN, Math.round(width)));
}

export function useTasksDockWidth() {
  const [width, setWidth] = useState(getTasksDockWidth);

  useEffect(() => {
    persistTasksDockWidth(width);
  }, [width]);

  const onResizeStart = useCallback(
    (event: React.MouseEvent) => {
      event.preventDefault();
      const startX = event.clientX;
      const startWidth = width;

      document.body.classList.add("tasks-dock-resizing");

      const onMove = (moveEvent: MouseEvent) => {
        const delta = startX - moveEvent.clientX;
        setWidth(clampWidth(startWidth + delta));
      };

      const onUp = () => {
        document.body.classList.remove("tasks-dock-resizing");
        window.removeEventListener("mousemove", onMove);
        window.removeEventListener("mouseup", onUp);
      };

      window.addEventListener("mousemove", onMove);
      window.addEventListener("mouseup", onUp);
    },
    [width],
  );

  return { width, onResizeStart };
}
