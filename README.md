# Aginxium

> Aginx 的引擎，类似 Chromium 之余 Chrome。

Aginxium 是 aginx 体系的客户端核心库，负责与 aginx 服务端通信、管理 Agent 会话、处理协议。

它不关心谁在用它——可能是 Android App，可能是企微机器人，可能是 CLI，可能什么界面都没有。

## 定位

```
aginx (nginx)        ← 服务端，路由 Agent 消息
aginxium (chromium)  ← 引擎，客户端核心库
aginx-app (chrome)   ← 产品，用户能看到的界面（由平台方实现）
```

## 分层架构

```
┌──────────────────────────────────────────────┐
│  调用方                                       │
│  Android App / 企微机器人 / CLI / ...         │
├──────────────────────────────────────────────┤
│  event          事件系统                      │  ← 唯一对外通道
├──────────────────────────────────────────────┤
│  agent          Agent 发现、注册、元数据       │
│  session        会话创建、对话、流式响应、关闭  │
├──────────────────────────────────────────────┤
│  connection     管理与一个 aginx 实例的连接    │
├──────────────────────────────────────────────┤
│  protocol       JSON-RPC 2.0 编解码           │
├──────────────────────────────────────────────┤
│  transport      TCP / Relay / WebSocket       │
└──────────────────────────────────────────────┘
```

每一层只依赖下一层，不跨层。

## 模块结构

```
aginxium/
├── Cargo.toml
├── src/
│   ├── lib.rs                  # 公开 API、re-export
│   │
│   ├── transport/              # 传输层
│   │   ├── mod.rs              # Transport trait 定义
│   │   ├── tcp.rs              # 直连 TCP
│   │   ├── relay.rs            # Relay 中继连接
│   │   └── websocket.rs        # WebSocket（预留）
│   │
│   ├── protocol/               # 协议层
│   │   ├── mod.rs              # Protocol trait 定义
│   │   ├── jsonrpc.rs          # JSON-RPC 2.0 编解码
│   │   └── acp.rs              # ACP 类型（Session、Agent、Permission 等）
│   │
│   ├── connection/             # 连接层
│   │   ├── mod.rs              # AginxConnection
│   │   └── manager.rs          # ConnectionManager（多连接管理）
│   │
│   ├── session/                # 会话层
│   │   ├── mod.rs              # Session 生命周期
│   │   └── stream.rs           # 流式响应处理
│   │
│   ├── agent/                  # Agent 层
│   │   ├── mod.rs              # Agent 操作
│   │   └── discovery.rs        # Agent 发现与注册
│   │
│   ├── event.rs                # 事件类型定义
│   └── error.rs                # 统一错误类型
```

---

## 核心抽象

### Transport（传输层）

传输层只做一件事：**在两端之间搬字节**。不关心内容，不关心协议。

```rust
#[async_trait]
pub trait Transport: Send + Sync {
    /// 发送原始字节
    async fn send(&self, data: &[u8]) -> Result<()>;

    /// 接收原始字节
    async fn recv(&self) -> Result<Vec<u8>>;

    /// 是否已连接
    fn is_connected(&self) -> bool;

    /// 关闭连接
    async fn close(&self) -> Result<()>;
}
```

实现：
- `TcpTransport` — 直连 `host:86`
- `RelayTransport` — 通过 `relay.aginx.net` 中继
- `WebSocketTransport` — 预留

### Protocol（协议层）

协议层只做一件事：**把结构化消息和字节互相转换**。不关心网络。

```rust
#[async_trait]
pub trait Protocol: Send + Sync {
    /// 编码请求为字节
    fn encode_request(&self, request: Request) -> Result<Vec<u8>>;

    /// 解码字节为响应
    fn decode_response(&self, data: &[u8]) -> Result<ProtocolMessage>;

    /// 编码通知为字节
    fn encode_notification(&self, notification: Notification) -> Result<Vec<u8>>;
}
```

`ProtocolMessage` 是 JSON-RPC 的统一消息类型，可能是 Response，也可能是 Notification。

### Connection（连接层）

把 Transport 和 Protocol 组合起来，管理一个 aginx 实例的连接。

```rust
pub struct AginxConnection {
    transport: Box<dyn Transport>,
    // 内部状态：请求 ID 计数器、回调映射等
}

impl AginxConnection {
    /// 连接到 aginx 实例
    pub async fn connect(url: &str) -> Result<Self>;

    /// 发送请求并等待响应
    pub async fn request(&self, method: &str, params: Value) -> Result<Response>;

    /// 订阅通知
    pub fn subscribe(&self) -> broadcast::Receiver<Notification>;

    /// 断开连接
    pub async fn disconnect(&self) -> Result<()>;
}
```

URL 解析：
- `agent://host:port` → TcpTransport
- `agent://id.relay.domain` → RelayTransport
- `ws://host:port` → WebSocketTransport

### Session（会话层）

```rust
pub struct Session {
    pub id: String,
    pub agent_id: String,
    pub connection: Arc<AginxConnection>,
}

impl Session {
    /// 创建会话
    pub async fn create(conn: &AginxConnection, agent_id: &str, cwd: Option<&str>) -> Result<Self>;

    /// 加载已有会话
    pub async fn load(conn: &AginxConnection, session_id: &str) -> Result<Self>;

    /// 发送消息，返回流式事件接收器
    pub async fn prompt(&self, message: &str) -> Result<Receiver<SessionEvent>>;

    /// 取消当前生成
    pub async fn cancel(&self) -> Result<()>;

    /// 关闭会话
    pub async fn close(self) -> Result<()>;
}
```

`prompt()` 返回一个 `Receiver<SessionEvent>`，调用方自行消费流式事件：

```rust
let mut stream = session.prompt("你好").await?;
while let Some(event) = stream.recv().await {
    match event {
        SessionEvent::TextChunk(text) => { /* 追加文字 */ }
        SessionEvent::ToolCallStart(tool) => { /* 工具调用开始 */ }
        SessionEvent::ToolCallUpdate(update) => { /* 工具调用进度 */ }
        SessionEvent::Done => break,
        SessionEvent::Error(e) => { /* 错误处理 */ }
    }
}
```

### Agent（Agent 层）

```rust
impl AginxConnection {
    /// 列出已注册的 Agent
    pub async fn list_agents(&self) -> Result<Vec<AgentInfo>>;

    /// 扫描发现 Agent
    pub async fn discover_agents(&self, path: &str) -> Result<Vec<DiscoveredAgent>>;

    /// 注册 Agent
    pub async fn register_agent(&self, agent: &DiscoveredAgent) -> Result<AgentInfo>;

    /// 获取 Agent 帮助信息
    pub async fn get_agent_help(&self, agent_id: &str) -> Result<String>;

    /// 绑定设备（配对码）
    pub async fn bind_device(&self, pair_code: &str, device_name: &str) -> Result<()>;
}
```

---

## 事件系统

事件是 aginxium 对外的**唯一输出通道**。调用方不需要轮询，不需要回调，只需要订阅事件流。

```rust
/// 所有事件类型
pub enum Event {
    /// 连接状态变化
    ConnectionChanged(ConnectionState),

    /// 会话事件（流式文本、工具调用等）
    SessionEvent { session_id: String, event: SessionEvent },

    /// 权限请求
    PermissionRequest(PermissionRequest),

    /// Agent 列表更新
    AgentsUpdated(Vec<AgentInfo>),
}

pub enum ConnectionState {
    Connecting,
    Connected,
    Disconnected,
    Reconnecting,
}

pub enum SessionEvent {
    TextChunk(String),
    ToolCallStart(ToolCall),
    ToolCallUpdate(ToolCallUpdate),
    AvailableCommands(Vec<String>),
    Done,
    Error(String),
}

pub struct PermissionRequest {
    pub session_id: String,
    pub tool_name: String,
    pub description: String,
    pub options: Vec<PermissionOption>,
}
```

调用方使用方式：

```rust
let client = AginxClient::connect("agent://rcs0aj94.relay.yinnho.cn").await?;
let mut events = client.subscribe();

tokio::spawn(async move {
    while let Ok(event) = events.recv().await {
        match event {
            Event::SessionEvent { session_id, event: SessionEvent::TextChunk(text) } => {
                print!("{text}");
            }
            Event::PermissionRequest(req) => {
                // 展示给用户，或者自动批准
                client.respond_permission(&req, "allow").await?;
            }
            _ => {}
        }
    }
});
```

---

## 对外 API 总览

`AginxClient` 是对外的统一入口：

```rust
pub struct AginxClient {
    // 内部持有 ConnectionManager
}

impl AginxClient {
    // ── 连接 ──
    pub async fn connect(url: &str) -> Result<Self>;
    pub async fn disconnect(&self) -> Result<()>;
    pub fn subscribe(&self) -> broadcast::Receiver<Event>;

    // ── Agent ──
    pub async fn list_agents(&self) -> Result<Vec<AgentInfo>>;
    pub async fn discover_agents(&self, path: &str) -> Result<Vec<DiscoveredAgent>>;
    pub async fn register_agent(&self, agent: &DiscoveredAgent) -> Result<AgentInfo>;
    pub async fn bind_device(&self, pair_code: &str, device_name: &str) -> Result<()>;

    // ── 会话 ──
    pub async fn create_session(&self, agent_id: &str, cwd: Option<&str>) -> Result<Session>;
    pub async fn load_session(&self, session_id: &str) -> Result<Session>;

    // ── 对话 ──
    pub async fn list_conversations(&self, agent_id: &str) -> Result<Vec<Conversation>>;
    pub async fn delete_conversation(&self, agent_id: &str, conversation_id: &str) -> Result<()>;

    // ── LLM 配置 ──
    pub async fn get_llm_config(&self) -> Result<LlmConfig>;
    pub async fn set_llm_config(&self, config: &LlmConfig) -> Result<()>;

    // ── 文件 ──
    pub async fn list_directory(&self, path: &str) -> Result<Vec<DirectoryEntry>>;
    pub async fn read_file(&self, path: &str) -> Result<String>;

    // ── 权限 ──
    pub async fn respond_permission(&self, request: &PermissionRequest, choice: &str) -> Result<()>;
}
```

---

## 调用方示例

### 企微机器人

```rust
use aginxium::AginxClient;

#[tokio::main]
async fn main() -> Result<()> {
    let client = AginxClient::connect("agent://rcs0aj94.relay.yinnho.cn").await?;

    // 监听事件
    let mut events = client.subscribe();
    tokio::spawn(async move {
        while let Ok(event) = events.recv().await {
            if let Event::PermissionRequest(req) = event {
                // 自动批准所有权限
                client.respond_permission(&req, "allow").await.unwrap();
            }
        }
    });

    // 创建会话并发消息
    let session = client.create_session("claude", None).await?;
    let mut stream = session.prompt("查看服务器磁盘空间").await?;

    let mut response = String::new();
    while let Some(event) = stream.recv().await {
        if let SessionEvent::TextChunk(text) = event {
            response.push_str(&text);
        }
        if let SessionEvent::Done = event {
            break;
        }
    }

    // 发送到企微
    wework_send_message(&response).await;
    session.close().await?;

    Ok(())
}
```

### Android App (通过 FFI)

Rust 侧暴露 C API，Kotlin 侧通过 JNI 调用：

```rust
// aginxium-ffi (单独 crate)
#[no_mangle]
pub extern "C" fn aginxium_connect(url: *const c_char) -> i64 { ... }

#[no_mangle]
pub extern "C" fn aginxium_session_prompt(handle: i64, session_id: *const c_char, message: *const c_char) -> i32 { ... }
```

Kotlin 侧自行处理 UI 渲染、Markdown 显示等。

---

## 依赖

```toml
[dependencies]
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "1"
anyhow = "1"
tokio-tungstenite = "0.21"    # WebSocket（预留）
tracing = "0.1"
async-trait = "0.1"
```

不引入任何 UI、平台相关的依赖。纯 Rust，纯逻辑。
