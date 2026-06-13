export type SseReplayMarkerPayload = {
  status?: string;
  last_seq?: number;
  last_replayed_seq?: number | null;
  gap?: boolean;
  /**
   * 表示这个 Run 历史上存在可回放事件，不表示本次请求一定还有新事件。
   */
  replay_available?: boolean;
  /**
   * 表示 Run 是否仍可能在运行；`stream_unavailable` + true 代表当前无法接入实时内存流。
   */
  live_stream?: boolean;
  after_seq?: number | null;
  message?: string;
};

export type SseEventPayload = SseReplayMarkerPayload & {
  content?: string;
  tool_name?: string;
  name?: string;
  args?: Record<string, unknown>;
  task_key?: string;
  worker_agent?: string;
  sub_agent?: boolean;
  elicitation_id?: string;
  request_type?: string;
  url?: string;
  id?: string;
  filename?: string;
  mime_type?: string;
  size_bytes?: number;
};

export type SseEvent = {
  type?: string;
  event_type?: string;
  run_id?: string;
  seq?: number;
  payload?: SseEventPayload;
};

export type SseEventType =
  | "agent_activity"
  | "artifact_created"
  | "awaiting_user"
  | "done"
  | "elicitation_request"
  | "error"
  | "stream_ended"
  | "stream_gap"
  | "stream_unavailable"
  | "sub_agent_cancelled"
  | "sub_agent_progress"
  | "tasks_updated"
  | "text"
  | "tool_call"
  | "tool_confirm_request";

export function sseEventType(event: SseEvent): SseEventType | string | undefined {
  return event.type ?? event.event_type;
}
