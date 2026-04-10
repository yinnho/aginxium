//! # Aginxium
//!
//! Aginx 客户端引擎 —— Agent Protocol 的 Chromium。
//!
//! 纯 Rust 实现，不含任何 UI 依赖。提供连接管理、会话管理、Agent 操作等核心能力。
//!
//! ## 快速开始
//!
//! ```no_run
//! use aginxium::AginxClient;
//!
//! #[tokio::main]
//! async fn main() -> aginxium::Result<()> {
//!     let client = AginxClient::connect("agent://192.168.1.100").await?;
//!
//!     // 创建会话并发消息
//!     let session = client.create_session("claude", None).await?;
//!     let mut stream = session.prompt("你好").await?;
//!
//!     while let Some(event) = stream.next().await {
//!         match event {
//!             aginxium::SessionEvent::TextChunk { text } => print!("{}", text),
//!             aginxium::SessionEvent::Done => break,
//!             _ => {}
//!         }
//!     }
//!
//!     session.close().await?;
//!     client.disconnect().await?;
//!     Ok(())
//! }
//! ```

pub mod agent;
pub mod connection;
pub mod error;
pub mod event;
pub mod protocol;
pub mod session;
pub mod transport;

#[cfg(feature = "ffi")]
pub mod ffi;

// UniFFI scaffolding（替代手动 UniFfiTag）
#[cfg(feature = "ffi")]
uniffi::setup_scaffolding!();

// Re-export 主要类型
pub use connection::AginxConnection;
pub use connection::manager::ConnectionManager;
pub use error::{AginxiumError, Result};
pub use event::{
    AgentInfo, Event, PermissionRequest, SessionEvent,
    ConnectionState, ToolCall, ToolCallUpdate, ToolCallState,
};
pub use protocol::acp::{
    AgentInfo as AcpAgentInfo, Conversation, DiscoveredAgent,
    DirectoryEntry, FileContent, PromptMessage, SessionInfo,
};
pub use session::{Session, SessionStream};

use std::sync::Arc;

/// 对外统一入口
pub struct AginxClient {
    conn: Arc<AginxConnection>,
}

impl AginxClient {
    /// 连接到 aginx 实例
    pub async fn connect(url: &str) -> Result<Self> {
        let conn = AginxConnection::connect(url).await?;
        Ok(Self {
            conn: Arc::new(conn),
        })
    }

    /// 订阅事件流
    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<Event> {
        self.conn.subscribe()
    }

    /// 获取内部连接引用
    pub fn connection(&self) -> &Arc<AginxConnection> {
        &self.conn
    }

    // ── 连接 ──

    /// 断开连接
    pub async fn disconnect(&self) -> Result<()> {
        self.conn.disconnect().await
    }

    // ── Agent ──

    /// 列出已注册的 Agent
    pub async fn list_agents(&self) -> Result<Vec<AcpAgentInfo>> {
        self.conn.list_agents().await
    }

    /// 扫描发现 Agent
    pub async fn discover_agents(&self, path: &str) -> Result<Vec<DiscoveredAgent>> {
        self.conn.discover_agents(path).await
    }

    /// 注册 Agent
    pub async fn register_agent(&self, config_path: &str) -> Result<AcpAgentInfo> {
        self.conn.register_agent(config_path).await
    }

    /// 绑定设备
    pub async fn bind_device(&self, pair_code: &str, device_name: &str) -> Result<()> {
        self.conn.bind_device(pair_code, device_name).await
    }

    // ── 会话 ──

    /// 创建新会话
    pub async fn create_session(&self, agent_id: &str, cwd: Option<&str>) -> Result<Session> {
        Session::create(self.conn.clone(), agent_id, cwd).await
    }

    /// 加载已有会话
    pub async fn load_session(&self, session_id: &str) -> Result<Session> {
        Session::load(self.conn.clone(), session_id).await
    }

    // ── 对话 ──

    /// 列出对话
    pub async fn list_conversations(&self, agent_id: &str) -> Result<Vec<Conversation>> {
        self.conn.list_conversations(agent_id).await
    }

    /// 删除对话
    pub async fn delete_conversation(&self, agent_id: &str, conversation_id: &str) -> Result<()> {
        self.conn.delete_conversation(agent_id, conversation_id).await
    }

    // ── 文件 ──

    /// 列出目录
    pub async fn list_directory(&self, path: &str) -> Result<Vec<DirectoryEntry>> {
        self.conn.list_directory(path).await
    }

    /// 读取文件
    pub async fn read_file(&self, path: &str) -> Result<FileContent> {
        self.conn.read_file(path).await
    }

    // ── 权限 ──

    /// 回复权限请求
    pub async fn respond_permission(&self, request_id: &str, option_id: &str) -> Result<()> {
        self.conn.respond_permission(request_id, option_id).await
    }
}
