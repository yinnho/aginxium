use crate::connection::AginxConnection;
use crate::error::Result;
use crate::protocol::acp::{AgentInfo, DiscoveredAgent, DirectoryEntry, Conversation, LlmConfig};

/// Agent 相关操作
impl AginxConnection {
    /// 列出已注册的 Agent
    pub async fn list_agents(&self) -> Result<Vec<AgentInfo>> {
        let result = self.request("listAgents", None).await?;
        let agents: Vec<AgentInfo> = if result.is_array() {
            serde_json::from_value(result)
                .map_err(|e| crate::error::AginxiumError::Protocol(format!("解析 Agent 列表失败: {}", e)))?
        } else {
            // 服务端返回 {"agents": [...]}
            result.get("agents")
                .cloned()
                .map(|v| serde_json::from_value(v)
                    .map_err(|e| crate::error::AginxiumError::Protocol(format!("解析 Agent 列表失败: {}", e))))
                .transpose()?
                .unwrap_or_default()
        };
        Ok(agents)
    }

    /// 扫描发现 Agent
    pub async fn discover_agents(&self, path: &str) -> Result<Vec<DiscoveredAgent>> {
        let params = serde_json::json!({ "path": path });
        let result = self.request("discoverAgents", Some(params)).await?;
        let agents: Vec<DiscoveredAgent> = serde_json::from_value(result)
            .map_err(|e| crate::error::AginxiumError::Protocol(format!("解析发现结果失败: {}", e)))?;
        Ok(agents)
    }

    /// 注册 Agent
    pub async fn register_agent(&self, agent: &DiscoveredAgent) -> Result<AgentInfo> {
        let params = serde_json::to_value(agent)
            .map_err(|e| crate::error::AginxiumError::Protocol(format!("序列化 Agent 失败: {}", e)))?;
        let result = self.request("registerAgent", Some(params)).await?;
        let info: AgentInfo = serde_json::from_value(result)
            .map_err(|e| crate::error::AginxiumError::Protocol(format!("解析注册结果失败: {}", e)))?;
        Ok(info)
    }

    /// 绑定设备（配对码）
    pub async fn bind_device(&self, pair_code: &str, device_name: &str) -> Result<()> {
        let params = serde_json::json!({
            "pairCode": pair_code,
            "deviceName": device_name,
        });
        self.request("bindDevice", Some(params)).await?;
        Ok(())
    }

    // ── LLM 配置 ──

    /// 获取 LLM 配置
    pub async fn get_llm_config(&self) -> Result<LlmConfig> {
        let result = self.request("getLLMConfig", None).await?;
        let config: LlmConfig = serde_json::from_value(result)
            .map_err(|e| crate::error::AginxiumError::Protocol(format!("解析 LLM 配置失败: {}", e)))?;
        Ok(config)
    }

    /// 设置 LLM 配置
    pub async fn set_llm_config(&self, config: &LlmConfig) -> Result<()> {
        let params = serde_json::to_value(config)
            .map_err(|e| crate::error::AginxiumError::Protocol(format!("序列化 LLM 配置失败: {}", e)))?;
        self.request("setLLMConfig", Some(params)).await?;
        Ok(())
    }

    // ── 对话 ──

    /// 列出对话
    pub async fn list_conversations(&self, agent_id: &str) -> Result<Vec<Conversation>> {
        let params = serde_json::json!({ "agentId": agent_id });
        let result = self.request("listConversations", Some(params)).await?;
        let conversations: Vec<Conversation> = serde_json::from_value(result)
            .map_err(|e| crate::error::AginxiumError::Protocol(format!("解析对话列表失败: {}", e)))?;
        Ok(conversations)
    }

    /// 删除对话
    pub async fn delete_conversation(&self, agent_id: &str, conversation_id: &str) -> Result<()> {
        let params = serde_json::json!({
            "agentId": agent_id,
            "conversationId": conversation_id,
        });
        self.request("deleteConversation", Some(params)).await?;
        Ok(())
    }

    // ── 文件 ──

    /// 列出目录
    pub async fn list_directory(&self, path: &str) -> Result<Vec<DirectoryEntry>> {
        let params = serde_json::json!({ "path": path });
        let result = self.request("listDirectory", Some(params)).await?;
        let entries: Vec<DirectoryEntry> = serde_json::from_value(result)
            .map_err(|e| crate::error::AginxiumError::Protocol(format!("解析目录列表失败: {}", e)))?;
        Ok(entries)
    }

    /// 读取文件
    pub async fn read_file(&self, path: &str) -> Result<String> {
        let params = serde_json::json!({ "path": path });
        let result = self.request("readFile", Some(params)).await?;
        let content = result["content"]
            .as_str()
            .unwrap_or("")
            .to_string();
        Ok(content)
    }

    // ── 权限 ──

    /// 回复权限请求
    pub async fn respond_permission(&self, session_id: &str, tool_name: &str, choice: &str) -> Result<()> {
        let params = serde_json::json!({
            "sessionId": session_id,
            "toolName": tool_name,
            "choice": choice,
        });
        self.request("permissionResponse", Some(params)).await?;
        Ok(())
    }
}
