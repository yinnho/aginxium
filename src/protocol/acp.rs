use serde::{Deserialize, Serialize};
use serde_json::Value;

// ── Agent ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInfo {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub capabilities: Vec<String>,
    #[serde(alias = "type")]
    pub agent_type: String,
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
    pub session_id: String,
    pub agent_id: String,
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

// ── Session Update 通知 ──

#[derive(Debug, Clone, Deserialize)]
pub struct SessionUpdate {
    pub session_id: String,
    #[serde(flatten)]
    pub kind: SessionUpdateKind,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum SessionUpdateKind {
    #[serde(rename = "agent_message_chunk")]
    TextChunk { text: String },
    #[serde(rename = "tool_call")]
    ToolCall { tool_call: ToolCallData },
    #[serde(rename = "tool_call_update")]
    ToolCallUpdate { update: ToolCallUpdateData },
    #[serde(rename = "available_commands")]
    AvailableCommands { commands: Vec<String> },
}

#[derive(Debug, Clone, Deserialize)]
pub struct ToolCallData {
    pub id: String,
    pub name: String,
    pub input: Value,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ToolCallUpdateData {
    pub id: String,
    pub output: Option<String>,
    pub error: Option<String>,
    pub state: Option<String>,
}

// ── Permission ──

#[derive(Debug, Clone, Deserialize)]
pub struct PermissionNotification {
    pub session_id: String,
    pub tool_name: String,
    pub description: String,
    pub options: Vec<PermissionOptionData>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PermissionOptionData {
    pub id: String,
    pub label: String,
}

// ── LLM Config ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub provider: String,
    pub api_key: String,
    pub endpoint: String,
    pub format: String,
    pub model: String,
    #[serde(default = "default_modality")]
    pub modality: String,
}

fn default_modality() -> String {
    "chat".to_string()
}

// ── Conversation ──

#[derive(Debug, Clone, Deserialize)]
pub struct Conversation {
    pub id: String,
    pub title: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

// ── Directory / File ──

#[derive(Debug, Clone, Deserialize)]
pub struct DirectoryEntry {
    pub name: String,
    #[serde(rename = "type")]
    pub entry_type: String,
    pub size: Option<u64>,
}
