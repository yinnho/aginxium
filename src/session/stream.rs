use tokio::sync::broadcast;

use crate::event::{Event, SessionEvent};

/// 会话流式响应接收器
///
/// 从全局事件流中过滤出指定 session_id 的事件
pub struct SessionStream {
    session_id: String,
    receiver: broadcast::Receiver<Event>,
    done: bool,
}

impl SessionStream {
    pub fn new(session_id: String, receiver: broadcast::Receiver<Event>) -> Self {
        Self {
            session_id,
            receiver,
            done: false,
        }
    }

    /// 接收下一个会话事件
    ///
    /// 返回 None 表示流已结束（收到 Done 或 Error）
    pub async fn next(&mut self) -> Option<SessionEvent> {
        if self.done {
            return None;
        }

        loop {
            match self.receiver.recv().await {
                Ok(Event::SessionEvent { session_id, event }) => {
                    if session_id != self.session_id {
                        continue; // 跳过其他会话的事件
                    }
                    if matches!(event, SessionEvent::Done | SessionEvent::Error { .. }) {
                        self.done = true;
                    }
                    return Some(event);
                }
                Ok(Event::PermissionRequest(_req)) => {
                    // 权限请求不属于 SessionEvent，跳过
                    // 调用方应通过 AginxClient::subscribe() 处理权限
                    continue;
                }
                Ok(_) => continue,
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("事件流落后 {} 条消息", n);
                    continue;
                }
                Err(broadcast::error::RecvError::Closed) => {
                    self.done = true;
                    return None;
                }
            }
        }
    }

    /// 收集完整响应文本（阻塞直到 Done）
    pub async fn collect_text(&mut self) -> String {
        let mut text = String::new();
        while let Some(event) = self.next().await {
            if let SessionEvent::TextChunk { text: chunk } = event {
                text.push_str(&chunk);
            }
        }
        text
    }
}
