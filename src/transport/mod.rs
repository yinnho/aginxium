pub mod relay;
pub mod tcp;

use async_trait::async_trait;
use crate::error::Result;

/// 传输层 trait —— 只负责在两端之间搬字节
#[async_trait]
pub trait Transport: Send + Sync {
    /// 发送原始字节
    async fn send(&self, data: &[u8]) -> Result<()>;

    /// 接收一行（以 \n 分隔的 JSON-RPC 消息）
    async fn recv_line(&self) -> Result<String>;

    /// 是否已连接
    fn is_connected(&self) -> bool;

    /// 关闭连接
    async fn close(&self) -> Result<()>;
}
