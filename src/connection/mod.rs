pub mod manager;

use std::sync::Arc;
use tokio::sync::{broadcast, Mutex, RwLock};

use crate::error::{AginxiumError, Result};
use crate::event::{ConnectionState, Event, SessionEvent};
use crate::protocol::acp::*;
use crate::protocol::jsonrpc::*;
use crate::transport::relay::{RelayOptions, RelayTransport};
use crate::transport::tcp::TcpTransport;
use crate::transport::Transport;

/// 连接参数
enum ConnectionParams {
    Direct { host: String, port: u16 },
    Relay {
        relay_host: String,
        relay_port: u16,
        target_id: String,
        use_tls: bool,
        tls_domain: String,
    },
}

/// 与一个 aginx 实例的连接
///
/// 组合 Transport + Protocol，管理请求-响应和通知分发
pub struct AginxConnection {
    transport: Arc<RwLock<Box<dyn Transport>>>,
    id_gen: Arc<IdGenerator>,
    pending: Arc<Mutex<std::collections::HashMap<u64, tokio::sync::oneshot::Sender<Response>>>>,
    /// request_id -> session_id，用于追踪 streaming prompt 的最终响应
    streaming_sessions: Arc<Mutex<std::collections::HashMap<u64, String>>>,
    event_tx: broadcast::Sender<Event>,
    state: Arc<RwLock<ConnectionState>>,
    recv_handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    reconnect_url: Arc<RwLock<Option<String>>>,
}

impl AginxConnection {
    /// 连接到 aginx 实例
    pub async fn connect(url: &str) -> Result<Self> {
        let params = parse_agent_url(url)?;
        let transport = create_transport(params).await?;

        let (event_tx, _) = broadcast::channel(256);

        let conn = Self {
            transport: Arc::new(RwLock::new(transport)),
            id_gen: Arc::new(IdGenerator::new()),
            pending: Arc::new(Mutex::new(std::collections::HashMap::new())),
            streaming_sessions: Arc::new(Mutex::new(std::collections::HashMap::new())),
            event_tx,
            state: Arc::new(RwLock::new(ConnectionState::Connected)),
            recv_handle: Arc::new(Mutex::new(None)),
            reconnect_url: Arc::new(RwLock::new(None)),
        };

        // 启动接收循环
        conn.start_recv_loop().await;

        tracing::info!("已连接到 aginx: {}", url);
        Ok(conn)
    }

    /// 连接并启用自动重连
    pub async fn connect_with_reconnect(url: &str) -> Result<Self> {
        let conn = Self::connect(url).await?;
        {
            let mut reconnect_url = conn.reconnect_url.write().await;
            *reconnect_url = Some(url.to_string());
        }
        Ok(conn)
    }

    /// 发送 JSON-RPC 请求并等待响应
    pub async fn request(&self, method: &str, params: Option<serde_json::Value>) -> Result<serde_json::Value> {
        let id = self.id_gen.next();
        let data = encode_request(id, method, params)?;
        self.transport.read().await.send(data.as_bytes()).await?;

        let (tx, rx) = tokio::sync::oneshot::channel();
        {
            let mut pending = self.pending.lock().await;
            pending.insert(id, tx);
        }

        let response = tokio::time::timeout(std::time::Duration::from_secs(60), rx).await
            .map_err(|_| AginxiumError::Timeout)?
            .map_err(|_| AginxiumError::Connection("连接关闭，等待响应失败".to_string()))?;

        extract_result(response)
    }

    /// 发送 streaming 请求（如 session/prompt）
    /// 等待初始 {"streaming": true} 响应，后续通过通知接收
    pub async fn request_streaming(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
        session_id: &str,
    ) -> Result<()> {
        let id = self.id_gen.next();
        let data = encode_request(id, method, params)?;
        self.transport.read().await.send(data.as_bytes()).await?;

        // 注册 oneshot 等待初始响应
        let (tx, rx) = tokio::sync::oneshot::channel();
        {
            let mut pending = self.pending.lock().await;
            pending.insert(id, tx);
        }

        let response = tokio::time::timeout(std::time::Duration::from_secs(30), rx).await
            .map_err(|_| AginxiumError::Timeout)?
            .map_err(|_| AginxiumError::Connection("连接关闭".to_string()))?;

        let result = extract_result(response)?;

        // 注册 streaming session 以追踪最终响应
        if result.get("streaming").and_then(|v| v.as_bool()) == Some(true) {
            let mut streaming = self.streaming_sessions.lock().await;
            streaming.insert(id, session_id.to_string());
        }
        Ok(())
    }

    /// 订阅事件流
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.event_tx.subscribe()
    }

    /// 获取连接状态
    pub async fn state(&self) -> ConnectionState {
        self.state.read().await.clone()
    }

    /// 断开连接（不触发重连）
    pub async fn disconnect(&self) -> Result<()> {
        // 清除 reconnect_url 防止重连
        {
            let mut url = self.reconnect_url.write().await;
            *url = None;
        }

        // 标记状态
        {
            let mut state = self.state.write().await;
            *state = ConnectionState::Disconnected;
        }

        // Abort recv loop
        {
            let mut recv_handle = self.recv_handle.lock().await;
            if let Some(handle) = recv_handle.take() {
                handle.abort();
            }
        }

        self.transport.read().await.close().await?;

        // 通知订阅者
        let _ = self.event_tx.send(Event::ConnectionChanged(ConnectionState::Disconnected));

        // 清除等待中的请求
        let mut pending = self.pending.lock().await;
        pending.clear();

        tracing::info!("已断开连接");
        Ok(())
    }

    /// 获取事件发送端
    pub fn event_sender(&self) -> &broadcast::Sender<Event> {
        &self.event_tx
    }

    // ── 内部方法 ──

    async fn start_recv_loop(&self) {
        let handle = spawn_recv_loop(
            self.transport.clone(),
            self.pending.clone(),
            self.streaming_sessions.clone(),
            self.event_tx.clone(),
            self.state.clone(),
            self.reconnect_url.clone(),
        ).await;

        let mut recv = self.recv_handle.lock().await;
        if let Some(old) = recv.take() {
            old.abort();
        }
        *recv = Some(handle);
    }
}

/// 启动 recv loop（独立函数避免 borrow 问题）
async fn spawn_recv_loop(
    transport: Arc<RwLock<Box<dyn Transport>>>,
    pending: Arc<Mutex<std::collections::HashMap<u64, tokio::sync::oneshot::Sender<Response>>>>,
    streaming_sessions: Arc<Mutex<std::collections::HashMap<u64, String>>>,
    event_tx: broadcast::Sender<Event>,
    state: Arc<RwLock<ConnectionState>>,
    reconnect_url: Arc<RwLock<Option<String>>>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            let line = {
                let t = transport.read().await;
                t.recv_line().await
            };
            match line {
                Ok(line) => {
                    if line.is_empty() {
                        continue;
                    }
                    match decode_message(&line) {
                        Ok(IncomingMessage::Response(response)) => {
                            if let Some(id) = response.id {
                                let mut pending_map = pending.lock().await;
                                if let Some(sender) = pending_map.remove(&id) {
                                    let _ = sender.send(response);
                                } else {
                                    drop(pending_map);
                                    let mut streaming = streaming_sessions.lock().await;
                                    if let Some(session_id) = streaming.remove(&id) {
                                        drop(streaming);
                                        emit_streaming_final(&event_tx, &session_id, &response);
                                    }
                                }
                            }
                        }
                        Ok(IncomingMessage::Notification(notification)) => {
                            let event = map_notification_to_event(&notification.method, &notification.params);
                            if let Some(event) = event {
                                let _ = event_tx.send(event);
                            }
                        }
                        Err(e) => {
                            tracing::warn!("解析消息失败: {} - {}", e, line);
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("接收失败: {}", e);
                    let mut state_guard = state.write().await;
                    *state_guard = ConnectionState::Disconnected;
                    drop(state_guard);
                    let _ = event_tx.send(Event::ConnectionChanged(ConnectionState::Disconnected));

                    // 尝试重连
                    let should_reconnect = reconnect_url.read().await.is_some();
                    if should_reconnect {
                        // 重连：替换 transport，在外部用 reconnect 不能递归 spawn，
                        // 所以简化为直接 break，让上层处理
                        if let Err(e) = try_replace_transport(&reconnect_url, &transport, &state, &event_tx).await {
                            tracing::error!("重连失败: {}", e);
                            break;
                        }
                        // 重连成功，继续 recv loop（transport 已被替换）
                        continue;
                    } else {
                        break;
                    }
                }
            }
        }
    })
}

/// 简化重连：只替换 transport，不重启 recv loop
async fn try_replace_transport(
    reconnect_url: &Arc<RwLock<Option<String>>>,
    transport: &Arc<RwLock<Box<dyn Transport>>>,
    state: &Arc<RwLock<ConnectionState>>,
    event_tx: &broadcast::Sender<Event>,
) -> Result<()> {
    let url = reconnect_url.read().await.clone();
    let url = match url {
        Some(u) => u,
        None => return Err(AginxiumError::Connection("无重连 URL".to_string())),
    };

    {
        let mut s = state.write().await;
        *s = ConnectionState::Reconnecting;
    }
    let _ = event_tx.send(Event::ConnectionChanged(ConnectionState::Reconnecting));

    let mut delay = std::time::Duration::from_secs(1);
    let max_delay = std::time::Duration::from_secs(30);

    loop {
        tracing::info!("{}秒后重连 {}...", delay.as_secs(), url);
        tokio::time::sleep(delay).await;

        if reconnect_url.read().await.is_none() {
            return Err(AginxiumError::Connection("重连已取消".to_string()));
        }

        match parse_agent_url(&url) {
            Ok(params) => {
                match create_transport(params).await {
                    Ok(new_transport) => {
                        {
                            let mut t = transport.write().await;
                            *t = new_transport;
                        }
                        tracing::info!("重连成功!");
                        {
                            let mut s = state.write().await;
                            *s = ConnectionState::Connected;
                        }
                        let _ = event_tx.send(Event::ConnectionChanged(ConnectionState::Connected));
                        return Ok(());
                    }
                    Err(e) => {
                        tracing::warn!("重连失败: {}", e);
                        delay = (delay * 2).min(max_delay);
                    }
                }
            }
            Err(e) => {
                return Err(e);
            }
        }
    }
}

/// 创建 transport
async fn create_transport(params: ConnectionParams) -> Result<Box<dyn Transport>> {
    match params {
        ConnectionParams::Direct { host, port } => {
            Ok(Box::new(TcpTransport::connect(&host, port).await?))
        }
        ConnectionParams::Relay { relay_host, relay_port, target_id, use_tls, tls_domain } => {
            let opts = RelayOptions {
                host: relay_host,
                port: relay_port,
                target_id,
                use_tls,
                tls_domain: Some(tls_domain),
            };
            RelayTransport::connect(opts)
                .await
                .map(|t| Box::new(t) as Box<dyn Transport>)
        }
    }
}

/// 解析 agent:// URL
///
/// 支持格式:
/// - agent://id.relay.yinnho.cn          → TLS relay, port 8443
/// - agent://id.relay.yinnho.cn:8600     → plain relay
/// - agent://host:port                   → 直连
/// - agent://host                        → 直连, 默认 port 86
fn parse_agent_url(url: &str) -> Result<ConnectionParams> {
    let url = url.trim();

    if let Some(rest) = url.strip_prefix("agent://") {
        // 检查 relay 模式：xxx.relay.xxx
        let parts: Vec<&str> = rest.split('.').collect();
        if parts.len() >= 4 && parts[1] == "relay" {
            let target_id = parts[0].to_string();

            // 提取 host:port 部分（去掉 target_id. 前缀）
            let host_port = rest.split_once('.')
                .map(|(_, h)| h)
                .unwrap_or(rest);

            // 分离 host 和 port
            let (relay_host, relay_port) = if let Some(colon) = host_port.rfind(':') {
                let host = &host_port[..colon];
                let port: u16 = host_port[colon + 1..].parse()
                    .map_err(|_| AginxiumError::InvalidUrl(format!("无效端口: {}", &host_port[colon + 1..])))?;
                (host.to_string(), port)
            } else {
                // 无端口 → 默认 TLS, port 8443
                (host_port.to_string(), 8443)
            };

            // TLS domain: 取 relay.xxx 部分（匹配 *.xxx 证书）
            let tls_domain = parts[1..].join(".");
            // 去掉端口后缀
            let tls_domain = tls_domain.split(':').next().unwrap_or(&tls_domain).to_string();

            let use_tls = relay_port == 8443;

            return Ok(ConnectionParams::Relay {
                relay_host,
                relay_port,
                target_id,
                use_tls,
                tls_domain,
            });
        }

        // 直连模式
        if let Some(colon) = rest.rfind(':') {
            let host = &rest[..colon];
            let port: u16 = rest[colon + 1..].parse()
                .map_err(|_| AginxiumError::InvalidUrl(format!("无效端口: {}", &rest[colon + 1..])))?;
            Ok(ConnectionParams::Direct { host: host.to_string(), port })
        } else {
            Ok(ConnectionParams::Direct { host: rest.to_string(), port: 86 })
        }
    } else {
        Err(AginxiumError::InvalidUrl(format!("不支持的 URL 格式: {}", url)))
    }
}

/// 解析 streaming 最终响应并发出事件
fn emit_streaming_final(
    event_tx: &broadcast::Sender<Event>,
    session_id: &str,
    response: &Response,
) {
    if let Some(result) = &response.result {
        let stop_reason = result.get("stopReason")
            .and_then(|v| v.as_str())
            .unwrap_or("end_turn");
        if stop_reason == "error" {
            let msg = result.get("response")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown error");
            let _ = event_tx.send(Event::SessionEvent {
                session_id: session_id.to_string(),
                event: SessionEvent::Error { message: msg.to_string() },
            });
        } else {
            let _ = event_tx.send(Event::SessionEvent {
                session_id: session_id.to_string(),
                event: SessionEvent::Done,
            });
        }
    } else if let Some(err) = &response.error {
        let _ = event_tx.send(Event::SessionEvent {
            session_id: session_id.to_string(),
            event: SessionEvent::Error { message: err.message.clone() },
        });
    }
}

/// 将 JSON-RPC 通知映射为 Event
fn map_notification_to_event(method: &str, params: &serde_json::Value) -> Option<Event> {
    match method {
        "sessionUpdate" => {
            let notif = serde_json::from_value::<SessionUpdateNotification>(params.clone()).ok()?;
            let event = match notif.update {
                ServerSessionUpdate::AgentMessageChunk { content: MessageContent::Text { text } } => {
                    SessionEvent::TextChunk { text }
                }
                ServerSessionUpdate::ToolCall { toolCallId, title, status: _, rawInput, .. } => {
                    SessionEvent::ToolCallStart {
                        tool_call: crate::event::ToolCall {
                            id: toolCallId,
                            name: title,
                            input: rawInput.unwrap_or(serde_json::Value::Null),
                        },
                    }
                }
                ServerSessionUpdate::ToolCallUpdate { toolCallId, status, rawOutput, content } => {
                    let output_text = rawOutput
                        .and_then(|v| if v.is_string() { v.as_str().map(String::from) } else { Some(v.to_string()) })
                        .or_else(|| {
                            content.and_then(|c| c.first().and_then(|cc| {
                                if let ToolCallContent::Content { content: MessageContent::Text { text } } = cc {
                                    Some(text.clone())
                                } else {
                                    None
                                }
                            }))
                        });
                    SessionEvent::ToolCallUpdate {
                        update: crate::event::ToolCallUpdate {
                            id: toolCallId,
                            output: output_text,
                            error: None,
                            state: match status {
                                Some(ToolCallStatus::Completed) => crate::event::ToolCallState::Completed,
                                Some(ToolCallStatus::Failed) => crate::event::ToolCallState::Failed,
                                Some(ToolCallStatus::InProgress) => crate::event::ToolCallState::Running,
                                None => crate::event::ToolCallState::Pending,
                            },
                        },
                    }
                }
                ServerSessionUpdate::AvailableCommandsUpdate { availableCommands } => {
                    SessionEvent::AvailableCommands {
                        commands: availableCommands.into_iter().map(|c| c.name).collect(),
                    }
                }
            };
            Some(Event::SessionEvent {
                session_id: notif.sessionId,
                event,
            })
        }
        "requestPermission" => {
            let req = serde_json::from_value::<PermissionNotification>(params.clone()).ok()?;
            Some(Event::PermissionRequest(crate::event::PermissionRequest {
                session_id: req.requestId,
                tool_name: req.toolCall
                    .and_then(|tc| tc.title)
                    .unwrap_or_default(),
                description: req.description.unwrap_or_default(),
                options: req.options.into_iter().map(|o| crate::event::PermissionOption {
                    id: o.optionId,
                    label: o.label,
                }).collect(),
            }))
        }
        _ => {
            tracing::debug!("忽略未知通知: {}", method);
            None
        }
    }
}
