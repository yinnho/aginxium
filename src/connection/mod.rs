pub mod manager;

use std::sync::Arc;
use tokio::sync::{broadcast, Mutex, RwLock};

use crate::error::{AginxiumError, Result};
use crate::event::{ConnectionState, Event, SessionEvent};
use crate::protocol::acp::*;
use crate::protocol::jsonrpc::*;
use crate::transport::tcp::TcpTransport;
use crate::transport::Transport;

/// 与一个 aginx 实例的连接
///
/// 组合 Transport + Protocol，管理请求-响应和通知分发
pub struct AginxConnection {
    transport: Arc<Box<dyn Transport>>,
    id_gen: Arc<IdGenerator>,
    pending: Arc<Mutex<std::collections::HashMap<u64, tokio::sync::oneshot::Sender<Response>>>>,
    event_tx: broadcast::Sender<Event>,
    state: Arc<RwLock<ConnectionState>>,
    recv_handle: Mutex<Option<tokio::task::JoinHandle<()>>>, // TODO: disconnect 时 abort
}

impl AginxConnection {
    /// 连接到 aginx 实例
    pub async fn connect(url: &str) -> Result<Self> {
        let (host, port) = parse_agent_url(url)?;

        let transport: Box<dyn Transport> = Box::new(TcpTransport::connect(&host, port).await?);

        let (event_tx, _) = broadcast::channel(256);

        let conn = Self {
            transport: Arc::new(transport),
            id_gen: Arc::new(IdGenerator::new()),
            pending: Arc::new(Mutex::new(std::collections::HashMap::new())),
            event_tx,
            state: Arc::new(RwLock::new(ConnectionState::Connected)),
            recv_handle: Mutex::new(None),
        };

        // 启动接收循环
        conn.start_recv_loop();

        tracing::info!("已连接到 aginx: {}", url);
        Ok(conn)
    }

    /// 发送 JSON-RPC 请求并等待响应
    pub async fn request(&self, method: &str, params: Option<serde_json::Value>) -> Result<serde_json::Value> {
        let id = self.id_gen.next();
        let data = encode_request(id, method, params)?;
        self.transport.send(data.as_bytes()).await?;

        // 注册 oneshot channel 等待响应
        let (tx, rx) = tokio::sync::oneshot::channel();
        {
            let mut pending = self.pending.lock().await;
            pending.insert(id, tx);
        }

        // 等待响应（带超时）
        let response = tokio::time::timeout(std::time::Duration::from_secs(60), rx).await
            .map_err(|_| AginxiumError::Timeout)?
            .map_err(|_| AginxiumError::Connection("连接关闭，等待响应失败".to_string()))?;

        extract_result(response)
    }

    /// 发送 JSON-RPC 请求但不等待响应（fire-and-forget）
    /// 用于 prompt 等通过通知返回结果的场景
    pub async fn notify(&self, method: &str, params: Option<serde_json::Value>) -> Result<()> {
        let id = self.id_gen.next();
        let data = encode_request(id, method, params)?;
        self.transport.send(data.as_bytes()).await?;
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

    /// 断开连接
    pub async fn disconnect(&self) -> Result<()> {
        {
            let mut state = self.state.write().await;
            *state = ConnectionState::Disconnected;
        }
        self.transport.close().await?;

        // 取消所有等待中的请求
        let mut pending = self.pending.lock().await;
        pending.clear();

        tracing::info!("已断开连接");
        Ok(())
    }

    /// 获取 transport 引用（供内部使用）
    pub fn transport(&self) -> &Arc<Box<dyn Transport>> {
        &self.transport
    }

    /// 获取事件发送端（供 ConnectionManager 使用）
    pub fn event_sender(&self) -> &broadcast::Sender<Event> {
        &self.event_tx
    }

    // ── 内部方法 ──

    fn start_recv_loop(&self) {
        let transport = self.transport.clone();
        let pending = self.pending.clone();
        let event_tx = self.event_tx.clone();
        let state = self.state.clone();

        let handle = tokio::spawn(async move {
            loop {
                match transport.recv_line().await {
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
                        let mut state = state.write().await;
                        *state = ConnectionState::Disconnected;
                        let _ = event_tx.send(Event::ConnectionChanged(ConnectionState::Disconnected));
                        break;
                    }
                }
            }
        });

        // 存储 handle（简化处理，不 await）
        // TODO: 在 disconnect 时 abort
        let _ = handle;
    }
}

/// 解析 agent:// URL
fn parse_agent_url(url: &str) -> Result<(String, u16)> {
    let url = url.trim();

    // agent://host:port
    if url.starts_with("agent://") {
        let rest = &url["agent://".len()..];
        if let Some(colon) = rest.rfind(':') {
            let host = &rest[..colon];
            let port: u16 = rest[colon + 1..].parse()
                .map_err(|_| AginxiumError::InvalidUrl(format!("无效端口: {}", &rest[colon + 1..])))?;
            Ok((host.to_string(), port))
        } else {
            // 默认端口 86
            Ok((rest.to_string(), 86))
        }
    } else {
        Err(AginxiumError::InvalidUrl(format!("不支持的 URL 格式: {}", url)))
    }
}

/// 将 JSON-RPC 通知映射为 Event
fn map_notification_to_event(method: &str, params: &serde_json::Value) -> Option<Event> {
    match method {
        "sessionUpdate" => {
            if let Ok(update) = serde_json::from_value::<SessionUpdate>(params.clone()) {
                let event = match update.kind {
                    SessionUpdateKind::TextChunk { text } => SessionEvent::TextChunk { text },
                    SessionUpdateKind::ToolCall { tool_call } => SessionEvent::ToolCallStart {
                        tool_call: crate::event::ToolCall {
                            id: tool_call.id,
                            name: tool_call.name,
                            input: tool_call.input,
                        },
                    },
                    SessionUpdateKind::ToolCallUpdate { update } => SessionEvent::ToolCallUpdate {
                        update: crate::event::ToolCallUpdate {
                            id: update.id,
                            output: update.output,
                            error: update.error,
                            state: match update.state.as_deref() {
                                Some("completed") => crate::event::ToolCallState::Completed,
                                Some("failed") => crate::event::ToolCallState::Failed,
                                Some("running") => crate::event::ToolCallState::Running,
                                _ => crate::event::ToolCallState::Pending,
                            },
                        },
                    },
                    SessionUpdateKind::AvailableCommands { commands } => {
                        SessionEvent::AvailableCommands { commands }
                    }
                };
                Some(Event::SessionEvent {
                    session_id: update.session_id,
                    event,
                })
            } else {
                None
            }
        }
        "requestPermission" => {
            if let Ok(req) = serde_json::from_value::<PermissionNotification>(params.clone()) {
                Some(Event::PermissionRequest(crate::event::PermissionRequest {
                    session_id: req.session_id,
                    tool_name: req.tool_name,
                    description: req.description,
                    options: req.options.into_iter().map(|o| crate::event::PermissionOption {
                        id: o.id,
                        label: o.label,
                    }).collect(),
                }))
            } else {
                None
            }
        }
        _ => {
            tracing::debug!("忽略未知通知: {}", method);
            None
        }
    }
}
