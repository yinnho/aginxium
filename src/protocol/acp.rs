// 字段名匹配服务器 JSON 格式（camelCase）
#![allow(non_snake_case)]

use serde::{Deserialize, Serialize};
use serde_json::Value;

// ── Agent ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInfo {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(alias = "type")]
    pub agent_type: String,
    #[serde(default)]
    pub require_workdir: Option<bool>,
    #[serde(default)]
    pub working_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredAgent {
    pub id: String,
    pub name: String,
    pub path: String,
    #[serde(alias = "type")]
    pub agent_type: Option<String>,
    pub capabilities: Option<Vec<String>>,
}

// ── Session ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub sessionId: String,
    #[serde(default)]
    pub agentId: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PromptMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub text: String,
}

impl PromptMessage {
    pub fn text(text: &str) -> Self {
        Self {
            msg_type: "text".to_string(),
            text: text.to_string(),
        }
    }
}

// ── Session Update 通知（匹配服务器格式）──

#[derive(Debug, Clone, Deserialize)]
pub struct SessionUpdateNotification {
    pub sessionId: String,
    pub update: ServerSessionUpdate,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "sessionUpdate")]
pub enum ServerSessionUpdate {
    #[serde(rename = "agent_message_chunk")]
    AgentMessageChunk { content: MessageContent },

    #[serde(rename = "tool_call")]
    ToolCall {
        toolCallId: String,
        title: String,
        status: ToolCallStatus,
        #[serde(default)]
        rawInput: Option<Value>,
        #[serde(default)]
        kind: Option<ToolKind>,
    },

    #[serde(rename = "tool_call_update")]
    ToolCallUpdate {
        toolCallId: String,
        #[serde(default)]
        status: Option<ToolCallStatus>,
        #[serde(default)]
        rawOutput: Option<Value>,
        #[serde(default)]
        content: Option<Vec<ToolCallContent>>,
    },

    #[serde(rename = "available_commands_update")]
    AvailableCommandsUpdate {
        availableCommands: Vec<CommandInfo>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum MessageContent {
    #[serde(rename = "text")]
    Text { text: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolCallStatus {
    InProgress,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolKind {
    Read,
    Edit,
    Delete,
    Move,
    Search,
    Execute,
    Fetch,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ToolCallContent {
    #[serde(rename = "content")]
    Content { content: MessageContent },
    #[serde(rename = "location")]
    Location {
        path: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        line: Option<u32>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandInfo {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

// ── Permission ──

#[derive(Debug, Clone, Deserialize)]
pub struct PermissionNotification {
    pub requestId: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub toolCall: Option<ToolCallInfo>,
    #[serde(default)]
    pub options: Vec<PermissionOptionData>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ToolCallInfo {
    pub toolCallId: String,
    #[serde(default)]
    pub title: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PermissionOptionData {
    pub optionId: String,
    pub label: String,
    #[serde(default)]
    pub kind: Option<String>,
}

// ── Conversation ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    #[serde(alias = "id")]
    pub sessionId: String,
    #[serde(default)]
    pub agentId: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub lastMessage: Option<String>,
    #[serde(default)]
    pub workdir: Option<String>,
    #[serde(default)]
    pub createdAt: Option<u64>,
    #[serde(default)]
    pub updatedAt: Option<u64>,
}

// ── Directory / File ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectoryEntry {
    pub name: String,
    #[serde(rename = "type")]
    pub entry_type: String,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub modified: Option<u64>,
    #[serde(default)]
    pub isHidden: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileContent {
    pub name: String,
    pub size: u64,
    pub content: String, // base64 encoded
    #[serde(default)]
    pub mimeType: Option<String>,
}
