use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::error::{AginxiumError, Result};

/// JSON-RPC 2.0 请求
#[derive(Debug, Serialize)]
pub struct Request {
    pub jsonrpc: &'static str,
    pub id: u64,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

/// JSON-RPC 2.0 响应
#[derive(Debug, Deserialize)]
pub struct Response {
    #[serde(default)]
    pub id: Option<u64>,
    pub result: Option<Value>,
    pub error: Option<RpcError>,
}

/// JSON-RPC 2.0 错误
#[derive(Debug, Deserialize, Clone)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
    #[serde(default)]
    pub data: Option<Value>,
}

/// JSON-RPC 2.0 通知（无 id，服务端主动推送）
#[derive(Debug, Deserialize)]
pub struct Notification {
    pub method: String,
    pub params: Value,
}

/// 收到的消息：可能是响应，也可能是通知
#[derive(Debug)]
pub enum IncomingMessage {
    Response(Response),
    Notification(Notification),
}

/// ID 生成器
pub struct IdGenerator {
    counter: AtomicU64,
}

impl IdGenerator {
    pub fn new() -> Self {
        Self {
            counter: AtomicU64::new(1),
        }
    }

    pub fn next(&self) -> u64 {
        self.counter.fetch_add(1, Ordering::Relaxed)
    }
}

// ── 编解码 ──

/// 编码请求为 JSON 字符串
pub fn encode_request(id: u64, method: &str, params: Option<Value>) -> Result<String> {
    let request = Request {
        jsonrpc: "2.0",
        id,
        method: method.to_string(),
        params,
    };
    serde_json::to_string(&request).map_err(|e| AginxiumError::Protocol(format!("编码请求失败: {}", e)))
}

/// 解码收到的原始字符串为消息
pub fn decode_message(data: &str) -> Result<IncomingMessage> {
    let value: Value = serde_json::from_str(data)
        .map_err(|e| AginxiumError::Protocol(format!("JSON 解析失败: {}", e)))?;

    if value.get("id").is_some() {
        // 有 id → 响应
        let response: Response = serde_json::from_value(value)
            .map_err(|e| AginxiumError::Protocol(format!("解析响应失败: {}", e)))?;
        Ok(IncomingMessage::Response(response))
    } else {
        // 无 id → 通知
        let notification: Notification = serde_json::from_value(value)
            .map_err(|e| AginxiumError::Protocol(format!("解析通知失败: {}", e)))?;
        Ok(IncomingMessage::Notification(notification))
    }
}

/// 从响应中提取 result，或将 RpcError 转为 AginxiumError
pub fn extract_result(response: Response) -> Result<Value> {
    if let Some(error) = response.error {
        Err(AginxiumError::Protocol(format!("[{}] {}", error.code, error.message)))
    } else {
        Ok(response.result.unwrap_or(Value::Null))
    }
}
