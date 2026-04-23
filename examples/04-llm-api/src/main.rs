use anyhow::Result;
use ex04_llm_api::{
    auto_provider_from_env, CompleteRequest, ContentBlock, Message, StreamEvent,
};
use futures::StreamExt;
use std::io::Write;

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let client = match auto_provider_from_env() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            return Ok(());
        }
    };

    // 模型名由环境变量决定，默认 Claude Opus。OpenAI 兼容环境下可设 MODEL=gpt-4o / deepseek-chat 等。
    let model = std::env::var("MODEL").unwrap_or_else(|_| "claude-opus-4-7".into());
    tracing::info!(%model, "using model");

    println!("--- non-streaming ---");
    let resp = client
        .complete(CompleteRequest {
            model: model.clone(),
            max_tokens: 256,
            messages: vec![Message::user("用一句话介绍 AI Agent。")],
            system: Some("你是一位简洁的技术讲师。".into()),
            temperature: Some(0.2),
            tools: None,
        })
        .await?;

    for block in &resp.content {
        if let ContentBlock::Text { text, .. } = block {
            println!("[assistant] {text}");
        }
    }
    println!("usage: {:?}", resp.usage);

    println!("\n--- streaming ---");
    let mut stream = client
        .stream(CompleteRequest {
            model,
            max_tokens: 256,
            messages: vec![Message::user("数到 5，每个数字换一行。")],
            system: None,
            temperature: Some(0.0),
            tools: None,
        })
        .await?;

    while let Some(event) = stream.next().await {
        match event? {
            StreamEvent::TextDelta(t) => {
                print!("{t}");
                std::io::stdout().flush()?;
            }
            StreamEvent::MessageStop { stop_reason, usage } => {
                println!("\n[stop={stop_reason}, usage={usage:?}]");
            }
            _ => {}
        }
    }
    Ok(())
}
