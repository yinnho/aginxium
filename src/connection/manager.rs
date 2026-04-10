use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::connection::AginxConnection;
use crate::event::Event;
use tokio::sync::broadcast;

/// 管理多个 aginx 连接
pub struct ConnectionManager {
    connections: Arc<RwLock<HashMap<String, Arc<AginxConnection>>>>,
    event_tx: broadcast::Sender<Event>,
}

impl ConnectionManager {
    pub fn new() -> Self {
        let (event_tx, _) = broadcast::channel(512);
        Self {
            connections: Arc::new(RwLock::new(HashMap::new())),
            event_tx,
        }
    }

    /// 添加一个连接，自动转发其事件到管理器的全局事件流
    pub async fn add(&self, name: String, conn: AginxConnection) {
        let conn = Arc::new(conn);

        // 转发连接事件到管理器的全局 event_tx
        let manager_tx = self.event_tx.clone();
        let mut rx = conn.subscribe();
        let name_clone = name.clone();
        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(event) => {
                        let _ = manager_tx.send(event);
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("[{}] 事件转发落后 {} 条，继续", name_clone, n);
                    }
                    Err(_) => break,
                }
            }
        });

        let mut connections = self.connections.write().await;
        connections.insert(name, conn);
    }

    /// 获取一个连接
    pub async fn get(&self, name: &str) -> Option<Arc<AginxConnection>> {
        let connections = self.connections.read().await;
        connections.get(name).cloned()
    }

    /// 移除一个连接
    pub async fn remove(&self, name: &str) -> Option<Arc<AginxConnection>> {
        let mut connections = self.connections.write().await;
        connections.remove(name)
    }

    /// 列出所有连接名称
    pub async fn list(&self) -> Vec<String> {
        let connections = self.connections.read().await;
        connections.keys().cloned().collect()
    }

    /// 订阅全局事件（聚合所有连接的事件）
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.event_tx.subscribe()
    }
}
