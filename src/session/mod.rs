pub mod stream;

use std::sync::Arc;

use crate::connection::AginxConnection;
use crate::error::{AginxiumError, Result};
use crate::protocol::acp::*;

pub use stream::SessionStream;

/// 与 Agent 的一个会话
pub struct Session {
    pub id: String,
    pub agent_id: String,
    conn: Arc<AginxConnection>,
}

impl Session {
    /// 创建新会话
    pub async fn create(conn: Arc<AginxConnection>, agent_id: &str, cwd: Option<&str>) -> Result<Self> {
        let mut params = serde_json::json!({
            "agentId": agent_id,
        });
        if let Some(cwd) = cwd {
            params["cwd"] = serde_json::Value::String(cwd.to_string());
        }

        let result = conn.request("session/new", Some(params)).await?;
        let session_id = result["sessionId"]
            .as_str()
            .ok_or_else(|| AginxiumError::Protocol("缺少 sessionId".to_string()))?
            .to_string();

        tracing::info!("会话已创建: {} (agent: {})", session_id, agent_id);

        Ok(Self {
            id: session_id,
            agent_id: agent_id.to_string(),
            conn,
        })
    }

    /// 加载已有会话
    pub async fn load(conn: Arc<AginxConnection>, session_id: &str) -> Result<Self> {
        let params = serde_json::json!({
            "sessionId": session_id,
        });

        let result = conn.request("session/load", Some(params)).await?;
        let agent_id = result["agentId"]
            .as_str()
            .unwrap_or("unknown")
            .to_string();

        tracing::info!("会话已加载: {}", session_id);

        Ok(Self {
            id: session_id.to_string(),
            agent_id,
            conn,
        })
    }

    /// 发送消息，返回流式事件接收器
    pub async fn prompt(&self, message: &str) -> Result<SessionStream> {
        let params = serde_json::json!({
            "sessionId": self.id,
            "prompt": [PromptMessage::text(message)],
        });

        // prompt 通过 sessionUpdate 通知返回流式数据，不等响应
        let stream = SessionStream::new(self.id.clone(), self.conn.subscribe());
        self.conn.notify("session/prompt", Some(params)).await?;

        Ok(stream)
    }

    /// 取消当前生成
    pub async fn cancel(&self) -> Result<()> {
        let params = serde_json::json!({
            "sessionId": self.id,
        });
        self.conn.request("session/cancel", Some(params)).await?;
        Ok(())
    }

    /// 关闭会话
    pub async fn close(self) -> Result<()> {
        let params = serde_json::json!({
            "sessionId": self.id,
        });
        self.conn.request("session/close", Some(params)).await?;
        tracing::info!("会话已关闭: {}", self.id);
        Ok(())
    }

    /// 获取会话 ID
    pub fn id(&self) -> &str {
        &self.id
    }

    /// 获取 Agent ID
    pub fn agent_id(&self) -> &str {
        &self.agent_id
    }
}
