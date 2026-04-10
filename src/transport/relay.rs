use async_trait::async_trait;

use crate::error::{AginxiumError, Result};
use super::tcp::TcpTransport;
use super::Transport;

/// Relay 中继传输
///
/// 通过 relay 服务器连接到远程 aginx 实例，支持 TLS
pub struct RelayTransport {
    inner: TcpTransport,
}

/// Relay 连接选项
pub struct RelayOptions {
    pub host: String,
    pub port: u16,
    pub target_id: String,
    pub use_tls: bool,
    pub tls_domain: Option<String>,
}

impl RelayTransport {
    /// 连接到 relay 并握手
    pub async fn connect(opts: RelayOptions) -> Result<Self> {
        // 创建 TCP 或 TLS 连接
        let inner = if opts.use_tls {
            let domain = opts.tls_domain.as_deref().unwrap_or(&opts.host);
            TcpTransport::connect_tls(&opts.host, opts.port, domain).await?
        } else {
            TcpTransport::connect(&opts.host, opts.port).await?
        };

        // 发送 connect 握手
        let connect_msg = serde_json::json!({
            "type": "connect",
            "target": opts.target_id
        });
        let msg = serde_json::to_string(&connect_msg)
            .map_err(|e| AginxiumError::Protocol(format!("序列化失败: {}", e)))?;
        inner.send(msg.as_bytes()).await?;

        // 读取握手响应
        let response_line = inner.recv_line().await?;
        let response: serde_json::Value = serde_json::from_str(&response_line)
            .map_err(|e| AginxiumError::Protocol(format!("无效握手响应: {}", e)))?;

        match response.get("type").and_then(|v| v.as_str()) {
            Some("connected") => {
                tracing::info!("Relay 已连接到目标: {} (TLS: {})", opts.target_id, opts.use_tls);
                Ok(Self { inner })
            }
            Some("error") => {
                let msg = response.get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("未知 relay 错误");
                Err(AginxiumError::Connection(msg.to_string()))
            }
            other => {
                Err(AginxiumError::Connection(
                    format!("意外的 relay 响应: {:?}", other)
                ))
            }
        }
    }
}

#[async_trait]
impl Transport for RelayTransport {
    async fn send(&self, data: &[u8]) -> Result<()> {
        self.inner.send(data).await
    }

    async fn recv_line(&self) -> Result<String> {
        // 跳过 ping/pong 心跳
        loop {
            let line = self.inner.recv_line().await?;
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&line) {
                match val.get("type").and_then(|v| v.as_str()) {
                    Some("ping") | Some("pong") => continue,
                    _ => return Ok(line),
                }
            } else {
                return Ok(line);
            }
        }
    }

    fn is_connected(&self) -> bool {
        self.inner.is_connected()
    }

    async fn close(&self) -> Result<()> {
        self.inner.close().await
    }
}
