import { sseEventType, type SseEvent, type SseEventType } from "../types/sse";

export type SseHandlerMap = Partial<Record<SseEventType | string, (event: SseEvent) => void>>;

export function dispatchSseEvent(event: SseEvent, handlers: SseHandlerMap) {
  const type = sseEventType(event);
  if (!type) return;
  handlers[type]?.(event);
}
