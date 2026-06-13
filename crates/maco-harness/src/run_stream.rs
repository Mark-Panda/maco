//! 活跃 Run 的 SSE 广播与 Runner 句柄注册（中断 / 重连订阅）。

use std::collections::HashMap;
use std::sync::Arc;

use adk_runner::Runner;
use maco_core::SseEnvelope;
use tokio::sync::{broadcast, Mutex};

const BROADCAST_CAP: usize = 256;

/// 单个会话当前活跃 Run 的流式出口。
pub struct ActiveRunHub {
    /// Run ID。
    pub run_id: String,
    /// SSE 广播发送端。
    pub tx: broadcast::Sender<SseEnvelope>,
    /// adk Runner（用于 interrupt）。
    pub runner: Arc<Runner>,
}

/// 按 `session_id` 索引活跃 Run，支持多订阅者 SSE 重连。
#[derive(Clone, Default)]
pub struct RunStreamRegistry {
    inner: Arc<Mutex<HashMap<String, ActiveRunHub>>>,
}

impl RunStreamRegistry {
    pub fn new() -> Self {
        Self::default()
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
            runner,
        };
        self.inner
            .lock()
            .await
            .insert(session_id.to_string(), hub);
        tx
    }

    /// 向活跃 Run 推送 SSE 事件。
    pub async fn publish(&self, session_id: &str, env: SseEnvelope) {
        if let Some(hub) = self.inner.lock().await.get(session_id) {
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

    /// 中断指定会话的 Runner 并返回 run_id。
    pub async fn interrupt(&self, session_id: &str) -> Option<String> {
        let hub = self.inner.lock().await.remove(session_id)?;
        hub.runner.interrupt(session_id);
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

}
