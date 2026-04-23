use anyhow::Result;
use ex04_llm_api::{AnthropicClient, CompleteRequest, ContentBlock, LlmProvider, Message};
use ex06_tool_use::{default_registry, ToolContext};

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt().with_env_filter("info").init();

    let registry = default_registry();
    let ctx = ToolContext {
        cwd: std::env::current_dir()?,
        session_id: uuid::Uuid::new_v4().to_string(),
        depth: 0,
    };

    println!("Registered tools:\n{}", serde_json::to_string_pretty(&registry.as_api_schema())?);

    if std::env::var("ANTHROPIC_API_KEY").is_err() {
        eprintln!("\nANTHROPIC_API_KEY not set — skipping live LLM call.");
        eprintln!("Running one local tool call for sanity:");
        let tool = registry.get("list_dir").unwrap();
        let out = tool
            .execute(serde_json::json!({"path": ".", "max_depth": 2}), &ctx)
            .await;
        println!("{}", out.content);
        return Ok(());
    }

    let client = AnthropicClient::from_env()?;
    let resp = client.complete(CompleteRequest {
        model: "claude-opus-4-7".into(),
        max_tokens: 1024,
        system: Some("你是一个文件系统助手。".into()),
        messages: vec![Message::user("列一下当前目录下有哪些东西。")],
        temperature: Some(0.0),
        tools: Some(registry.as_api_schema()),
    }).await?;

    for block in &resp.content {
        if let ContentBlock::ToolUse { id, name, input } = block {
            println!("tool_use: {name}({input})");
            if let Some(tool) = registry.get(name) {
                let out = tool.execute(input.clone(), &ctx).await;
                println!("=> error={} output:\n{}", out.is_error, out.content);
            }
            let _ = id;
        }
    }
    Ok(())
}
