//! 第 7 章：Agent Loop —— 完整的 tool-calling 循环。

use anyhow::{bail, Result};
use ex04_llm_api::{
    CompleteRequest, ContentBlock, LlmProvider, Message, Role, Usage,
};
use ex06_tool_use::{ToolContext, ToolRegistry};
use std::sync::Arc;

pub struct AgentLoop {
    pub llm: Arc<dyn LlmProvider>,
    pub registry: Arc<ToolRegistry>,
    pub ctx: ToolContext,
    pub system: String,
    pub model: String,
    pub max_tokens: u32,
    pub max_iterations: u32,
    pub temperature: f32,
}

pub struct AgentRun {
    pub final_text: String,
    pub messages: Vec<Message>,
    pub iterations: u32,
    pub total_usage: Usage,
}

impl AgentLoop {
    pub async fn run(&self, user_input: impl Into<String>) -> Result<AgentRun> {
        let mut messages = vec![Message::user(user_input)];
        let mut total = Usage::default();

        for iter in 1..=self.max_iterations {
            tracing::info!(iter, "loop iteration");

            let resp = self.llm.complete(CompleteRequest {
                model: self.model.clone(),
                max_tokens: self.max_tokens,
                messages: messages.clone(),
                system: Some(self.system.clone()),
                temperature: Some(self.temperature),
                tools: Some(self.registry.as_api_schema()),
            }).await?;

            total += resp.usage;

            messages.push(Message {
                role: Role::Assistant,
                content: resp.content.clone(),
            });

            let stop = resp.stop_reason.as_deref().unwrap_or("");
            match stop {
                "end_turn" | "stop_sequence" => {
                    let text = extract_text(&resp.content);
                    return Ok(AgentRun {
                        final_text: text,
                        messages,
                        iterations: iter,
                        total_usage: total,
                    });
                }
                "tool_use" => {
                    let tool_results = self.run_tool_calls(&resp.content).await;
                    messages.push(Message { role: Role::User, content: tool_results });
                }
                "max_tokens" => bail!("assistant hit max_tokens"),
                other => bail!("unexpected stop_reason: {other}"),
            }
        }
        bail!("exceeded max_iterations={}", self.max_iterations)
    }

    async fn run_tool_calls(&self, blocks: &[ContentBlock]) -> Vec<ContentBlock> {
        let calls: Vec<_> = blocks.iter().filter_map(|b| {
            if let ContentBlock::ToolUse { id, name, input } = b {
                Some((id.clone(), name.clone(), input.clone()))
            } else {
                None
            }
        }).collect();

        let mut futs = Vec::with_capacity(calls.len());
        for (id, name, input) in calls {
            let reg = self.registry.clone();
            let ctx = self.ctx.clone();
            futs.push(tokio::spawn(async move {
                let (content, is_error) = match reg.get(&name) {
                    Some(t) => {
                        let out = t.execute(input, &ctx).await;
                        (out.content, out.is_error)
                    }
                    None => (format!("unknown tool: {name}"), true),
                };
                ContentBlock::ToolResult {
                    tool_use_id: id,
                    content,
                    is_error,
                }
            }));
        }

        let mut results = Vec::new();
        for f in futs {
            if let Ok(r) = f.await {
                results.push(r);
            }
        }
        results
    }
}

fn extract_text(blocks: &[ContentBlock]) -> String {
    blocks.iter().filter_map(|b| {
        if let ContentBlock::Text { text, .. } = b {
            Some(text.as_str())
        } else {
            None
        }
    }).collect::<Vec<_>>().join("\n")
}
