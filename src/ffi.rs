//! FFI Facade for UniFFI bindings
//!
//! 提供干净的跨平台 API，通过 UniFFI 自动生成 Kotlin/Swift binding。
//! 核心引擎代码完全不变，这里只做薄包装层。

use std::sync::Arc;
use tokio::sync::Mutex;

use crate::event::{ConnectionState, Event};
use crate::error::AginxiumError;
use crate::AginxClient;

// ── FFI 错误类型 ──

/// FFI 错误
#[derive(Debug, uniffi::Enum)]
pub enum FfiError {
    Connection { description: String },
    Timeout,
    Disconnected,
    Protocol { description: String },
    SessionNotFound { description: String },
    AgentNotFound { description: String },
    InvalidUrl { description: String },
    Auth { description: String },
    Io { description: String },
    Json { description: String },
    Other { description: String },
}

impl From<AginxiumError> for FfiError {
    fn from(e: AginxiumError) -> Self {
        match e {
            AginxiumError::Connection(msg) => FfiError::Connection { description: msg },
            AginxiumError::Disconnected => FfiError::Disconnected,
            AginxiumError::Timeout => FfiError::Timeout,
            AginxiumError::Protocol(msg) => FfiError::Protocol { description: msg },
            AginxiumError::SessionNotFound(msg) => FfiError::SessionNotFound { description: msg },
            AginxiumError::AgentNotFound(msg) => FfiError::AgentNotFound { description: msg },
            AginxiumError::Auth(msg) => FfiError::Auth { description: msg },
            AginxiumError::InvalidUrl(msg) => FfiError::InvalidUrl { description: msg },
            AginxiumError::Io(e) => FfiError::Io { description: e.to_string() },
            AginxiumError::Json(e) => FfiError::Json { description: e.to_string() },
            AginxiumError::Other(msg) => FfiError::Other { description: msg },
        }
    }
}

impl std::fmt::Display for FfiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FfiError::Connection { description } => write!(f, "Connection: {}", description),
            FfiError::Timeout => write!(f, "Timeout"),
            FfiError::Disconnected => write!(f, "Disconnected"),
            FfiError::Protocol { description } => write!(f, "Protocol: {}", description),
            FfiError::SessionNotFound { description } => write!(f, "SessionNotFound: {}", description),
            FfiError::AgentNotFound { description } => write!(f, "AgentNotFound: {}", description),
            FfiError::Auth { description } => write!(f, "Auth: {}", description),
            FfiError::InvalidUrl { description } => write!(f, "InvalidUrl: {}", description),
            FfiError::Io { description } => write!(f, "Io: {}", description),
            FfiError::Json { description } => write!(f, "Json: {}", description),
            FfiError::Other { description } => write!(f, "Other: {}", description),
        }
    }
}

// ── FFI 事件类型 ──

/// 连接状态
#[derive(Debug, Clone, uniffi::Enum)]
pub enum FfiConnectionState {
    Connecting,
    Connected,
    Disconnected,
    Reconnecting,
}

impl From<ConnectionState> for FfiConnectionState {
    fn from(s: ConnectionState) -> Self {
        match s {
            ConnectionState::Connecting => FfiConnectionState::Connecting,
            ConnectionState::Connected => FfiConnectionState::Connected,
            ConnectionState::Disconnected => FfiConnectionState::Disconnected,
            ConnectionState::Reconnecting => FfiConnectionState::Reconnecting,
        }
    }
}

/// FFI 事件
#[derive(Debug, Clone, uniffi::Enum)]
pub enum FfiEvent {
    /// 连接状态变化
    ConnectionChanged { state: FfiConnectionState },
    /// 会话事件（text chunk, tool call 等，event_json 包含具体内容）
    SessionEvent { session_id: String, event_json: String },
    /// 权限请求
    PermissionRequest { request_json: String },
    /// Agent 列表更新
    AgentsUpdated { agents_json: String },
}

impl From<Event> for FfiEvent {
    fn from(event: Event) -> Self {
        match event {
            Event::ConnectionChanged(state) => FfiEvent::ConnectionChanged {
                state: state.into(),
            },
            Event::SessionEvent { session_id, event: se } => {
                let event_json = serde_json::to_string(&se).unwrap_or_default();
                FfiEvent::SessionEvent { session_id, event_json }
            }
            Event::PermissionRequest(req) => {
                let request_json = serde_json::to_string(&req).unwrap_or_default();
                FfiEvent::PermissionRequest { request_json }
            }
            Event::AgentsUpdated(agents) => {
                let agents_json = serde_json::to_string(&agents).unwrap_or_default();
                FfiEvent::AgentsUpdated { agents_json }
            }
        }
    }
}

// ── FFI 事件监听器 ──

/// 事件监听器（由 Kotlin/Swift 端实现）
#[uniffi::export(callback_interface)]
pub trait FfiEventListener: Send + Sync {
    /// 收到事件
    fn on_event(&self, event: FfiEvent);
}

// ── FFI 客户端 ──

/// FFI Aginx 客户端
#[derive(uniffi::Object)]
pub struct FfiAginxClient {
    inner: AginxClient,
    #[allow(dead_code)] // cloned into event task, not read directly
    listener: Arc<Mutex<Option<Box<dyn FfiEventListener>>>>,
    event_task: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
}

#[uniffi::export(async_runtime = "tokio")]
impl FfiAginxClient {
    /// 连接到 aginx 实例
    ///
    /// URL 格式:
    /// - agent://id.relay.yinnho.cn     (TLS relay)
    /// - agent://host:port              (直连)
    ///
    /// auth_token: 绑定设备时获得的 token，传空字符串表示无 token（首次绑定）
    #[uniffi::constructor]
    pub async fn connect(url: String, auth_token: String) -> Result<Self, FfiError> {
        let token = if auth_token.is_empty() { None } else { Some(auth_token.as_str()) };
        let client = AginxClient::connect(&url, token).await?;
        Ok(Self {
            inner: client,
            listener: Arc::new(Mutex::new(None)),
            event_task: Arc::new(Mutex::new(None)),
        })
    }

    /// 断开连接
    pub async fn disconnect(&self) -> Result<(), FfiError> {
        // 停止事件转发任务
        {
            let mut task = self.event_task.lock().await;
            if let Some(handle) = task.take() {
                handle.abort();
            }
        }
        self.inner.disconnect().await?;
        Ok(())
    }

    /// 设置事件监听器
    pub async fn set_event_listener(&self, listener: Box<dyn FfiEventListener>) {
        let listener = Arc::new(Mutex::new(Some(listener)));
        let mut rx = self.inner.subscribe();

        // Spawn 事件转发任务
        let handle = tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(event) => {
                        let ffi_event: FfiEvent = event.into();
                        let guard = listener.lock().await;
                        if let Some(ref l) = *guard {
                            l.on_event(ffi_event);
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("事件转发落后 {} 条", n);
                    }
                    Err(_) => break,
                }
            }
        });

        let mut task = self.event_task.lock().await;
        if let Some(old) = task.take() {
            old.abort();
        }
        *task = Some(handle);
    }

    /// 列出已注册的 Agent（返回 JSON 字符串）
    pub async fn list_agents(&self) -> Result<String, FfiError> {
        let agents = self.inner.list_agents().await?;
        Ok(serde_json::to_string(&agents)
            .map_err(|e| FfiError::Other { description: e.to_string() })?)
    }

    /// 创建新会话，返回 session_id
    pub async fn create_session(
        &self,
        agent_id: String,
        cwd: Option<String>,
    ) -> Result<String, FfiError> {
        let session = self.inner.create_session(&agent_id, cwd.as_deref()).await?;
        Ok(session.id().to_string())
    }

    /// 加载已有会话
    pub async fn load_session(&self, session_id: String) -> Result<(), FfiError> {
        self.inner.load_session(&session_id).await?;
        Ok(())
    }

    /// 发送 prompt（流式响应通过事件监听器回调）
    pub async fn prompt(&self, session_id: String, text: String) -> Result<(), FfiError> {
        // 需要先创建或加载 session，然后 prompt
        // 由于 Session 对象持有连接引用，这里需要特殊处理
        // 简化方案：直接通过 connection 发送请求
        let conn = self.inner.connection();
        let params = serde_json::json!({
            "sessionId": session_id,
            "prompt": [{"type": "text", "text": text}]
        });
        conn.request_streaming("session/prompt", Some(params), &session_id).await?;
        Ok(())
    }

    /// 取消会话
    pub async fn cancel_session(&self, session_id: String) -> Result<(), FfiError> {
        let conn = self.inner.connection();
        conn.request("session/cancel", Some(serde_json::json!({"sessionId": session_id}))).await?;
        Ok(())
    }

    /// 绑定设备，返回 token（用于后续连接认证）
    pub async fn bind_device(&self, pair_code: String, device_name: String) -> Result<String, FfiError> {
        let token = self.inner.bind_device(&pair_code, &device_name).await?;
        Ok(token)
    }

    /// 列出对话（返回 JSON 字符串）
    pub async fn list_conversations(&self, agent_id: String) -> Result<String, FfiError> {
        let conversations = self.inner.list_conversations(&agent_id).await?;
        Ok(serde_json::to_string(&conversations)
            .map_err(|e| FfiError::Other { description: e.to_string() })?)
    }

    /// 列出目录（返回 JSON 字符串）
    pub async fn list_directory(&self, path: String) -> Result<String, FfiError> {
        let entries = self.inner.list_directory(&path).await?;
        Ok(serde_json::to_string(&entries)
            .map_err(|e| FfiError::Other { description: e.to_string() })?)
    }

    /// 读取文件（返回 JSON 字符串）
    pub async fn read_file(&self, path: String) -> Result<String, FfiError> {
        let content = self.inner.read_file(&path).await?;
        Ok(serde_json::to_string(&content)
            .map_err(|e| FfiError::Other { description: e.to_string() })?)
    }

    /// 回复权限请求
    pub async fn respond_permission(
        &self,
        request_id: String,
        option_id: String,
    ) -> Result<(), FfiError> {
        self.inner.respond_permission(&request_id, &option_id).await?;
        Ok(())
    }

    /// 通用 JSON-RPC 请求，返回 JSON 字符串
    pub async fn raw_request(&self, method: String, params_json: Option<String>) -> Result<String, FfiError> {
        let params = match params_json {
            Some(ref s) => Some(serde_json::from_str(s).map_err(|e| FfiError::Other { description: e.to_string() })?),
            None => None,
        };
        let result = self.inner.connection().request(&method, params).await?;
        Ok(serde_json::to_string(&result).map_err(|e| FfiError::Other { description: e.to_string() })?)
    }
}
