use serde::{Deserialize, Serialize};

/// 事件系统对外输出的所有事件类型
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Event {
    /// 连接状态变化
    ConnectionChanged(ConnectionState),

    /// 会话事件（流式文本、工具调用等）
    SessionEvent {
        session_id: String,
        #[serde(flatten)]
        event: SessionEvent,
    },

    /// 权限请求
    PermissionRequest(PermissionRequest),

    /// Agent 列表更新
    AgentsUpdated(Vec<AgentInfo>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConnectionState {
    Connecting,
    Connected,
    Disconnected,
    Reconnecting,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum SessionEvent {
    /// 流式文本块
    TextChunk { text: String },
    /// 工具调用开始
    ToolCallStart { tool_call: ToolCall },
    /// 工具调用进度更新
    ToolCallUpdate { update: ToolCallUpdate },
    /// 可用命令更新
    AvailableCommands { commands: Vec<String> },
    /// 会话完成
    Done,
    /// 错误
    Error { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRequest {
    pub session_id: String,
    pub tool_name: String,
    pub description: String,
    pub options: Vec<PermissionOption>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionOption {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub input: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallUpdate {
    pub id: String,
    pub output: Option<String>,
    pub error: Option<String>,
    pub state: ToolCallState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ToolCallState {
    Pending,
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInfo {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub capabilities: Vec<String>,
    #[serde(rename = "type")]
    pub agent_type: String,
}
