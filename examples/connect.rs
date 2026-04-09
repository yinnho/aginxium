use aginxium::{AginxClient, SessionEvent};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let url = std::env::args().nth(1).unwrap_or_else(|| "agent://106.75.32.216".to_string());
    println!("连接到 {} ...", url);

    let client = AginxClient::connect(&url).await?;
    println!("已连接!");

    // 列出 Agent
    let agents = client.list_agents().await?;
    println!("\n已注册 Agent ({}):", agents.len());
    for agent in &agents {
        println!("  - {} ({}) [{}]", agent.name, agent.id, agent.capabilities.join(", "));
    }

    if agents.is_empty() {
        println!("没有可用的 Agent，跳过会话测试");
        client.disconnect().await?;
        return Ok(());
    }

    // 优先用 shell 或第一个支持会话的 agent
    let agent = agents.iter().find(|a| a.id == "shell")
        .or_else(|| agents.iter().find(|a| a.agent_type != "builtin"))
        .unwrap_or(&agents[0]);
    println!("\n使用 Agent: {} ({})\n", agent.name, agent.id);

    let session = client.create_session(&agent.id, None).await?;
    println!("会话已创建: {}", session.id());

    // 在另一个任务里监听事件
    let mut events = client.subscribe();
    let session_id = session.id().to_string();
    let event_handle = tokio::spawn(async move {
        loop {
            match events.recv().await {
                Ok(aginxium::Event::SessionEvent { session_id, event }) => {
                    if session_id != session_id { continue; }
                    match event {
                        SessionEvent::TextChunk { text } => print!("{}", text),
                        SessionEvent::ToolCallStart { tool_call } => {
                            println!("\n[工具调用] {} ({})", tool_call.name, tool_call.id);
                        }
                        SessionEvent::ToolCallUpdate { update } => {
                            if let Some(output) = update.output {
                                println!("\n[工具输出] {}", output);
                            }
                        }
                        SessionEvent::Done => {
                            println!("\n--- 完成 ---");
                            break;
                        }
                        SessionEvent::Error { message } => {
                            println!("\n[错误] {}", message);
                            break;
                        }
                        _ => {}
                    }
                }
                Ok(aginxium::Event::PermissionRequest(req)) => {
                    println!("\n[权限请求] {} - {}", req.tool_name, req.description);
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }
    });

    // 发送消息
    let prompt = std::env::args().nth(2).unwrap_or_else(|| "你是谁？简单介绍一下".to_string());
    println!("发送: {}\n", prompt);
    println!("--- 响应 ---");
    session.prompt(&prompt).await?;

    // 等待事件流结束
    event_handle.await?;

    session.close().await?;
    client.disconnect().await?;

    println!("\n已断开连接");
    Ok(())
}
