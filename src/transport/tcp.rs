use async_trait::async_trait;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::Mutex;

use crate::error::{AginxiumError, Result};
use super::Transport;

pub struct TcpTransport {
    writer: Mutex<tokio::io::WriteHalf<TcpStream>>,
    reader: Mutex<BufReader<tokio::io::ReadHalf<TcpStream>>>,
    connected: Mutex<bool>,
}

impl TcpTransport {
    pub async fn connect(host: &str, port: u16) -> Result<Self> {
        let stream = TcpStream::connect((host, port)).await
            .map_err(|e| AginxiumError::Connection(format!("无法连接到 {}:{} - {}", host, port, e)))?;

        let (read_half, write_half) = tokio::io::split(stream);
        let reader = BufReader::new(read_half);

        tracing::info!("TCP 已连接到 {}:{}", host, port);

        Ok(Self {
            writer: Mutex::new(write_half),
            reader: Mutex::new(reader),
            connected: Mutex::new(true),
        })
    }
}

#[async_trait]
impl Transport for TcpTransport {
    async fn send(&self, data: &[u8]) -> Result<()> {
        let mut writer = self.writer.lock().await;
        writer.write_all(data).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;
        Ok(())
    }

    async fn recv_line(&self) -> Result<String> {
        let mut reader = self.reader.lock().await;
        let mut line = String::new();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            *self.connected.lock().await = false;
            return Err(AginxiumError::Disconnected);
        }
        let line = line.trim_end_matches('\n').trim_end_matches('\r').to_string();
        Ok(line)
    }

    fn is_connected(&self) -> bool {
        // 同步检查，不能 await，用 try_lock
        match self.connected.try_lock() {
            Ok(guard) => *guard,
            Err(_) => true, // 锁被占用说明正在使用，视为已连接
        }
    }

    async fn close(&self) -> Result<()> {
        *self.connected.lock().await = false;
        let mut writer = self.writer.lock().await;
        writer.shutdown().await?;
        Ok(())
    }
}
