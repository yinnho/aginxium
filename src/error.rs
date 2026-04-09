use thiserror::Error;

#[derive(Error, Debug)]
pub enum AginxiumError {
    #[error("连接错误: {0}")]
    Connection(String),

    #[error("连接已断开")]
    Disconnected,

    #[error("请求超时")]
    Timeout,

    #[error("协议错误: {0}")]
    Protocol(String),

    #[error("会话不存在: {0}")]
    SessionNotFound(String),

    #[error("Agent 不存在: {0}")]
    AgentNotFound(String),

    #[error("认证失败: {0}")]
    Auth(String),

    #[error("URL 解析错误: {0}")]
    InvalidUrl(String),

    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON 错误: {0}")]
    Json(#[from] serde_json::Error),

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, AginxiumError>;
