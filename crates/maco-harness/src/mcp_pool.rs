//! MCP 连接池占位实现：引用计数 + 空闲超时标记，后续接入真实 MCP 客户端。

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::Mutex;

/// MCP 连接状态（当前为内存态，未持久化）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpConnState {
    /// 未连接。
    Disconnected,
    /// 连接中。
    Connecting,
    /// 可用。
    Ready,
    /// 发生错误。
    Error,
    /// 连接失败。
    Failed,
}

/// 单个 MCP 服务的池内条目。
struct McpEntry {
    /// 连接状态。
    state: McpConnState,
    /// 当前引用计数。
    refs: AtomicUsize,
    /// 最后使用时间。
    last_used: Mutex<Instant>,
}

/// 按 MCP 服务名管理连接引用与空闲回收。
pub struct McpPool {
    inner: Arc<McpPoolInner>,
}

struct McpPoolInner {
    /// 服务名 → 连接条目。
    entries: Mutex<HashMap<String, McpEntry>>,
    /// 空闲超时时间。
    idle_timeout: Duration,
}

impl McpPool {
    /// 创建空连接池，默认空闲超时 300 秒。
    pub fn new() -> Self {
        Self {
            inner: Arc::new(McpPoolInner {
                entries: Mutex::new(HashMap::new()),
                idle_timeout: Duration::from_secs(300),
            }),
        }
    }

    /// 获取指定 MCP 服务的连接守卫；`Drop` 时自动释放引用。
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

    /// 递减引用计数；无引用且超过空闲时间则标记为断开。
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

    /// 将连接标记为异常，下次 acquire 需重建。
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

/// RAII 守卫：`drop` 时异步归还连接池引用。
pub struct MacoConnGuard {
    /// 所属连接池。
    pool: Arc<McpPool>,
    /// MCP 服务名称。
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
