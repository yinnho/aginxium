use async_trait::async_trait;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, AsyncRead, AsyncWrite, BufReader};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio_rustls::TlsConnector;
use rustls_pki_types::ServerName;

use crate::error::{AginxiumError, Result};
use super::Transport;

/// TCP 传输（支持 TLS via rustls）
pub struct TcpTransport {
    writer: Mutex<Box<dyn AsyncWrite + Unpin + Send>>,
    reader: Mutex<BufReader<Box<dyn AsyncRead + Unpin + Send>>>,
    connected: Mutex<bool>,
}

impl TcpTransport {
    /// 纯 TCP 连接
    pub async fn connect(host: &str, port: u16) -> Result<Self> {
        let stream = TcpStream::connect((host, port)).await
            .map_err(|e| AginxiumError::Connection(format!("无法连接到 {}:{} - {}", host, port, e)))?;

        tracing::info!("TCP 已连接到 {}:{}", host, port);

        let (read_half, write_half) = tokio::io::split(stream);
        Self::from_halves(read_half, write_half)
    }

    /// TLS 连接（rustls，使用系统根证书）
    pub async fn connect_tls(host: &str, port: u16, domain: &str) -> Result<Self> {
        let stream = TcpStream::connect((host, port)).await
            .map_err(|e| AginxiumError::Connection(format!("无法连接到 {}:{} - {}", host, port, e)))?;

        let mut root_store = rustls::RootCertStore::empty();
        root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

        let config = rustls::ClientConfig::builder_with_provider(
            std::sync::Arc::new(rustls::crypto::ring::default_provider())
        )
        .with_safe_default_protocol_versions()
        .map_err(|e| AginxiumError::Connection(format!("TLS 版本错误: {}", e)))?
        .with_root_certificates(root_store)
        .with_no_client_auth();
        let connector = TlsConnector::from(std::sync::Arc::new(config));

        let server_name = ServerName::try_from(domain.to_string())
            .map_err(|e| AginxiumError::Connection(format!("无效的 TLS 域名: {}", e)))?;

        let tls_stream = connector.connect(server_name, stream)
            .await
            .map_err(|e| AginxiumError::Connection(format!("TLS 握手失败 ({}:{}): {}", host, port, e)))?;

        tracing::info!("TLS 已连接到 {}:{}", host, port);

        let (read_half, write_half) = tokio::io::split(tls_stream);
        Self::from_halves(read_half, write_half)
    }

    fn from_halves<R: AsyncRead + Unpin + Send + 'static, W: AsyncWrite + Unpin + Send + 'static>(
        read_half: R,
        write_half: W,
    ) -> Result<Self> {
        Ok(Self {
            writer: Mutex::new(Box::new(write_half)),
            reader: Mutex::new(BufReader::new(Box::new(read_half))),
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
        match self.connected.try_lock() {
            Ok(guard) => *guard,
            Err(_) => true,
        }
    }

    async fn close(&self) -> Result<()> {
        *self.connected.lock().await = false;
        let mut writer = self.writer.lock().await;
        writer.shutdown().await?;
        Ok(())
    }
}
