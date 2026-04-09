use crate::connection::AginxConnection;
use crate::error::{AginxiumError, Result};
use crate::protocol::acp::{AgentInfo, DiscoveredAgent, DirectoryEntry, Conversation, LlmConfig, FileContent};

/// 从 result 中提取数组字段（服务端返回 {"xxx": [...]} 格式）
fn extract_list<T: serde::de::DeserializeOwned>(result: serde_json::Value, field: &str) -> Result<Vec<T>> {
    if result.is_array() {
        serde_json::from_value(result)
            .map_err(|e| AginxiumError::Protocol(format!("解析列表失败: {}", e)))
    } else {
        result.get(field)
            .cloned()
            .map(|v| serde_json::from_value(v)
                .map_err(|e| AginxiumError::Protocol(format!("解析列表失败: {}", e))))
            .transpose()
            .map(|v| v.unwrap_or_default())
    }
}

/// Agent 相关操作
impl AginxConnection {
    /// 列出已注册的 Agent
    pub async fn list_agents(&self) -> Result<Vec<AgentInfo>> {
        let result = self.request("listAgents", None).await?;
        extract_list(result, "agents")
    }

    /// 扫描发现 Agent
    pub async fn discover_agents(&self, path: &str) -> Result<Vec<DiscoveredAgent>> {
        let params = serde_json::json!({ "path": path });
        let result = self.request("discoverAgents", Some(params)).await?;
        extract_list(result, "agents")
    }

    /// 注册 Agent
    pub async fn register_agent(&self, config_path: &str) -> Result<AgentInfo> {
        let params = serde_json::json!({ "configPath": config_path });
        let result = self.request("registerAgent", Some(params)).await?;
        let info: AgentInfo = serde_json::from_value(result)
            .map_err(|e| AginxiumError::Protocol(format!("解析注册结果失败: {}", e)))?;
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
            .map_err(|e| AginxiumError::Protocol(format!("解析 LLM 配置失败: {}", e)))?;
        Ok(config)
    }

    /// 设置 LLM 配置
    pub async fn set_llm_config(&self, config: &LlmConfig) -> Result<()> {
        let params = serde_json::to_value(config)
            .map_err(|e| AginxiumError::Protocol(format!("序列化 LLM 配置失败: {}", e)))?;
        self.request("setLLMConfig", Some(params)).await?;
        Ok(())
    }

    // ── 对话 ──

    /// 列出对话
    pub async fn list_conversations(&self, agent_id: &str) -> Result<Vec<Conversation>> {
        let params = serde_json::json!({ "agentId": agent_id });
        let result = self.request("listConversations", Some(params)).await?;
        extract_list(result, "conversations")
    }

    /// 删除对话（服务器用 sessionId 参数）
    pub async fn delete_conversation(&self, _agent_id: &str, conversation_id: &str) -> Result<()> {
        let params = serde_json::json!({
            "sessionId": conversation_id,
        });
        self.request("deleteConversation", Some(params)).await?;
        Ok(())
    }

    /// 列出会话
    pub async fn list_sessions(&self, agent_id: Option<&str>) -> Result<Vec<serde_json::Value>> {
        let params = agent_id.map(|id| serde_json::json!({ "agentId": id }));
        let result = self.request("listSessions", params).await?;
        extract_list(result, "sessions")
    }

    /// 获取消息
    pub async fn get_messages(&self, session_id: &str, limit: Option<u32>) -> Result<Vec<serde_json::Value>> {
        let mut params = serde_json::json!({ "sessionId": session_id });
        if let Some(limit) = limit {
            params["limit"] = serde_json::Value::Number(limit.into());
        }
        let result = self.request("getMessages", Some(params)).await?;
        extract_list(result, "messages")
    }

    // ── 文件 ──

    /// 列出目录
    pub async fn list_directory(&self, path: &str) -> Result<Vec<DirectoryEntry>> {
        let params = serde_json::json!({ "path": path });
        let result = self.request("listDirectory", Some(params)).await?;
        extract_list(result, "entries")
    }

    /// 读取文件（返回 base64 编码内容 + 元数据）
    pub async fn read_file(&self, path: &str) -> Result<FileContent> {
        let params = serde_json::json!({ "path": path });
        let result = self.request("readFile", Some(params)).await?;
        let content: FileContent = serde_json::from_value(result)
            .map_err(|e| AginxiumError::Protocol(format!("解析文件内容失败: {}", e)))?;
        Ok(content)
    }

    // ── 权限 ──

    /// 回复权限请求
    pub async fn respond_permission(&self, request_id: &str, option_id: &str) -> Result<()> {
        let params = serde_json::json!({
            "requestId": request_id,
            "optionId": option_id,
        });
        self.request("permissionResponse", Some(params)).await?;
        Ok(())
    }
}
