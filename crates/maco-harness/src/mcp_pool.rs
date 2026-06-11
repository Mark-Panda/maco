use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpConnState {
    Disconnected,
    Connecting,
    Ready,
    Error,
    Failed,
}

struct McpEntry {
    state: McpConnState,
    refs: AtomicUsize,
    last_used: Mutex<Instant>,
}

pub struct McpPool {
    inner: Arc<McpPoolInner>,
}

struct McpPoolInner {
    entries: Mutex<HashMap<String, McpEntry>>,
    idle_timeout: Duration,
}

impl McpPool {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(McpPoolInner {
                entries: Mutex::new(HashMap::new()),
                idle_timeout: Duration::from_secs(300),
            }),
        }
    }

    pub async fn acquire(self: &Arc<Self>, server_name: &str) -> MacoConnGuard {
        let mut map = self.inner.entries.lock().await;
        let entry = map.entry(server_name.to_string()).or_insert_with(|| McpEntry {
            state: McpConnState::Disconnected,
            refs: AtomicUsize::new(0),
            last_used: Mutex::new(Instant::now()),
        });
        entry.state = McpConnState::Ready;
        entry.refs.fetch_add(1, Ordering::SeqCst);
        MacoConnGuard {
            pool: Arc::clone(self),
            server_name: server_name.to_string(),
        }
    }

    async fn release(&self, server_name: &str) {
        let mut map = self.inner.entries.lock().await;
        if let Some(entry) = map.get_mut(server_name) {
            let prev = entry.refs.fetch_sub(1, Ordering::SeqCst);
            let inner = &self.inner;
            if prev <= 1 {
                entry.state = McpConnState::Disconnected;
            }
            let mut last = entry.last_used.lock().await;
            if inner.idle_timeout < last.elapsed() && entry.refs.load(Ordering::SeqCst) == 0 {
                entry.state = McpConnState::Disconnected;
            }
            *last = Instant::now();
        }
    }

    pub async fn mark_stale(&self, server_name: &str) {
        let mut map = self.inner.entries.lock().await;
        if let Some(entry) = map.get_mut(server_name) {
            entry.state = McpConnState::Error;
        }
    }
}

impl Default for McpPool {
    fn default() -> Self {
        Self::new()
    }
}

pub struct MacoConnGuard {
    pool: Arc<McpPool>,
    server_name: String,
}

impl Drop for MacoConnGuard {
    fn drop(&mut self) {
        let pool = Arc::clone(&self.pool);
        let name = self.server_name.clone();
        tokio::spawn(async move {
            pool.release(&name).await;
        });
    }
}
