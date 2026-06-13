//! 活跃 Run 的 SSE 广播与 Runner 句柄注册（中断 / 短断线重连订阅）。

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use adk_runner::Runner;
use maco_core::SseEnvelope;
use maco_db::RunEventRepo;
use tokio::sync::{Mutex, broadcast};

const BROADCAST_CAP: usize = 256;
const REPLAY_CAP: usize = 256;

/// 单个会话当前活跃 Run 的流式出口。
pub struct ActiveRunHub {
    /// Run ID。
    pub run_id: String,
    /// SSE 广播发送端。
    pub tx: broadcast::Sender<SseEnvelope>,
    /// adk Runner（用于 interrupt）。
    pub runner: Option<Arc<Runner>>,
    /// 最近 SSE 事件，用于短断线重连时按 seq 回放。
    pub replay: VecDeque<SseEnvelope>,
}

/// 活跃 Run 订阅结果：先发送 replay，再接实时广播。
pub struct RunStreamSubscription {
    pub session_id: String,
    pub run_id: String,
    pub replay_gap: bool,
    pub replay: Vec<SseEnvelope>,
    pub rx: broadcast::Receiver<SseEnvelope>,
}

/// 按 `session_id` 索引活跃 Run，支持多订阅者 SSE 重连。
#[derive(Clone, Default)]
pub struct RunStreamRegistry {
    inner: Arc<Mutex<HashMap<String, ActiveRunHub>>>,
    events: Option<RunEventRepo>,
}

impl RunStreamRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_event_repo(events: RunEventRepo) -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            events: Some(events),
        }
    }

    /// 注册新的活跃 Run；若已有旧 Run 则覆盖。
    pub async fn register(
        &self,
        session_id: &str,
        run_id: String,
        runner: Arc<Runner>,
    ) -> broadcast::Sender<SseEnvelope> {
        let (tx, _) = broadcast::channel(BROADCAST_CAP);
        let hub = ActiveRunHub {
            run_id: run_id.clone(),
            tx: tx.clone(),
            runner: Some(runner),
            replay: VecDeque::with_capacity(REPLAY_CAP),
        };
        self.inner.lock().await.insert(session_id.to_string(), hub);
        tx
    }

    /// 向活跃 Run 推送 SSE 事件。
    pub async fn publish(&self, session_id: &str, env: SseEnvelope) {
        if let Some(events) = &self.events
            && let Err(e) = events.append(&env).await
        {
            tracing::warn!("persist run SSE event {}#{}: {e}", env.run_id, env.seq);
        }
        if let Some(hub) = self.inner.lock().await.get_mut(session_id) {
            if hub.replay.len() == REPLAY_CAP {
                hub.replay.pop_front();
            }
            hub.replay.push_back(env.clone());
            let _ = hub.tx.send(env);
        }
    }

    /// 订阅活跃 Run 的 SSE 广播（用于重连）。
    pub async fn subscribe(
        &self,
        session_id: &str,
    ) -> Option<(String, broadcast::Receiver<SseEnvelope>)> {
        let map = self.inner.lock().await;
        map.get(session_id)
            .map(|h| (h.run_id.clone(), h.tx.subscribe()))
    }

    /// 订阅活跃 Run，并回放 `after_seq` 之后的内存事件。
    pub async fn subscribe_since(
        &self,
        session_id: &str,
        after_seq: Option<u64>,
    ) -> Option<RunStreamSubscription> {
        let (run_id, mut replay, rx) = {
            let map = self.inner.lock().await;
            let h = map.get(session_id)?;
            let rx = h.tx.subscribe();
            let replay: Vec<SseEnvelope> = h
                .replay
                .iter()
                .filter(|env| after_seq.is_none_or(|seq| env.seq > seq))
                .cloned()
                .collect();
            (h.run_id.clone(), replay, rx)
        };

        let mut replay_gap = false;
        if let Some(events) = &self.events {
            const PAGE_SIZE: u32 = 1_000;
            const MAX_REPLAY_EVENTS: usize = 10_000;

            let seq = after_seq.unwrap_or(0);
            let memory_starts_after_gap = replay
                .first()
                .is_some_and(|env| env.seq > seq.saturating_add(1));
            if memory_starts_after_gap || replay.is_empty() {
                let memory_first_seq = replay.first().map(|env| env.seq);
                let mut cursor = Some(seq);
                let mut db_replay = Vec::new();
                loop {
                    match events.list_after(&run_id, cursor, PAGE_SIZE).await {
                        Ok(page) if page.is_empty() => break,
                        Ok(page) => {
                            cursor = page.last().map(|env| env.seq);
                            db_replay.extend(page);
                            if db_replay.len() >= MAX_REPLAY_EVENTS
                                || db_replay.len() % PAGE_SIZE as usize != 0
                            {
                                break;
                            }
                            if let (Some(first), Some(last)) = (memory_first_seq, cursor)
                                && last >= first.saturating_sub(1)
                            {
                                break;
                            }
                        }
                        Err(e) => {
                            replay_gap = true;
                            tracing::warn!("replay run events from DB for {run_id}: {e}");
                            break;
                        }
                    }
                }

                if !db_replay.is_empty() {
                    let last_db_seq = db_replay.last().map(|env| env.seq).unwrap_or(seq);
                    replay_gap =
                        memory_first_seq.is_some_and(|first| last_db_seq < first.saturating_sub(1));
                    replay = db_replay;
                    replay.extend(
                        {
                            let map = self.inner.lock().await;
                            map.get(session_id)
                                .map(|h| {
                                    h.replay
                                        .iter()
                                        .filter(|env| env.seq > last_db_seq)
                                        .cloned()
                                        .collect::<Vec<_>>()
                                })
                                .unwrap_or_default()
                        }
                        .into_iter(),
                    );
                } else if memory_starts_after_gap {
                    replay_gap = true;
                }
            }
        }

        Some(RunStreamSubscription {
            session_id: session_id.to_string(),
            run_id,
            replay_gap,
            replay,
            rx,
        })
    }

    /// 中断指定会话的 Runner 并返回 run_id。
    pub async fn interrupt(&self, session_id: &str) -> Option<String> {
        let hub = self.inner.lock().await.remove(session_id)?;
        if let Some(runner) = hub.runner {
            runner.interrupt(session_id);
        }
        Some(hub.run_id)
    }

    /// Run 结束时移除注册。
    pub async fn unregister(&self, session_id: &str) {
        self.inner.lock().await.remove(session_id);
    }

    /// 查询活跃 run_id。
    pub async fn active_run_id(&self, session_id: &str) -> Option<String> {
        self.inner
            .lock()
            .await
            .get(session_id)
            .map(|h| h.run_id.clone())
    }

    /// 是否有任意会话正在执行 Run（用于 MCP 重载门禁）。
    pub async fn has_active(&self) -> bool {
        !self.inner.lock().await.is_empty()
    }

    #[cfg(test)]
    async fn register_for_test(&self, session_id: &str, run_id: &str) {
        let (tx, _) = broadcast::channel(BROADCAST_CAP);
        let hub = ActiveRunHub {
            run_id: run_id.to_string(),
            tx,
            runner: None,
            replay: VecDeque::with_capacity(REPLAY_CAP),
        };
        self.inner.lock().await.insert(session_id.to_string(), hub);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn active_run_id_empty_when_unregistered() {
        let reg = RunStreamRegistry::new();
        assert!(reg.active_run_id("session-1").await.is_none());
    }

    #[tokio::test]
    async fn subscribe_returns_none_without_active_run() {
        let reg = RunStreamRegistry::new();
        assert!(reg.subscribe("session-1").await.is_none());
    }

    #[tokio::test]
    async fn subscribe_since_replays_events_after_last_seq() {
        let reg = RunStreamRegistry::new();
        reg.register_for_test("session-1", "run-1").await;
        reg.publish(
            "session-1",
            SseEnvelope {
                event_type: "text".into(),
                run_id: "run-1".into(),
                seq: 1,
                payload: serde_json::json!({ "content": "old" }),
            },
        )
        .await;
        reg.publish(
            "session-1",
            SseEnvelope {
                event_type: "text".into(),
                run_id: "run-1".into(),
                seq: 2,
                payload: serde_json::json!({ "content": "new" }),
            },
        )
        .await;

        let sub = reg
            .subscribe_since("session-1", Some(1))
            .await
            .expect("active run");

        assert_eq!(sub.run_id, "run-1");
        assert_eq!(sub.session_id, "session-1");
        assert_eq!(sub.replay.len(), 1);
        assert_eq!(sub.replay[0].seq, 2);
    }

    #[tokio::test]
    async fn subscribe_since_without_seq_replays_buffered_events() {
        let reg = RunStreamRegistry::new();
        reg.register_for_test("session-1", "run-1").await;
        reg.publish(
            "session-1",
            SseEnvelope {
                event_type: "text".into(),
                run_id: "run-1".into(),
                seq: 1,
                payload: serde_json::json!({ "content": "first" }),
            },
        )
        .await;
        reg.publish(
            "session-1",
            SseEnvelope {
                event_type: "text".into(),
                run_id: "run-1".into(),
                seq: 2,
                payload: serde_json::json!({ "content": "second" }),
            },
        )
        .await;

        let sub = reg
            .subscribe_since("session-1", None)
            .await
            .expect("active run");

        assert_eq!(sub.replay.len(), 2);
        assert_eq!(sub.replay[0].seq, 1);
        assert_eq!(sub.replay[1].seq, 2);
    }
}
