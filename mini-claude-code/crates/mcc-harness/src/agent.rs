//! AgentLoop 骨架（完整版见第 23 章）。

use anyhow::{bail, Result};
use mcc_core::{ContentBlock, Message, Role, ToolContext, Usage};
use mcc_llm::{CompleteRequest, LlmProvider};
use mcc_tools::ToolRegistry;
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
    pub iterations: u32,
    pub total_usage: Usage,
}

impl AgentLoop {
    pub async fn run(&self, user_input: impl Into<String>) -> Result<AgentRun> {
        let mut messages = vec![Message::user(user_input)];
        let mut total = Usage::default();

        for iter in 1..=self.max_iterations {
            let resp = self
                .llm
                .complete(CompleteRequest {
                    model: self.model.clone(),
                    max_tokens: self.max_tokens,
                    messages: messages.clone(),
                    system: Some(self.system.clone()),
                    temperature: Some(self.temperature),
                    tools: Some(self.registry.as_api_schema()),
                })
                .await?;

            total.input_tokens += resp.usage.input_tokens;
            total.output_tokens += resp.usage.output_tokens;

            messages.push(Message {
                role: Role::Assistant,
                content: resp.content.clone(),
            });

            let stop = resp.stop_reason.as_deref().unwrap_or("end_turn");
            match stop {
                "end_turn" | "stop_sequence" => {
                    let text = resp
                        .content
                        .iter()
                        .filter_map(|b| {
                            if let ContentBlock::Text { text, .. } = b {
                                Some(text.as_str())
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    return Ok(AgentRun {
                        final_text: text,
                        iterations: iter,
                        total_usage: total,
                    });
                }
                "tool_use" => {
                    let results = self.run_tool_calls(&resp.content).await;
                    messages.push(Message { role: Role::User, content: results });
                }
                other => bail!("unexpected stop: {other}"),
            }
        }
        bail!("exceeded max_iterations")
    }

    async fn run_tool_calls(&self, blocks: &[ContentBlock]) -> Vec<ContentBlock> {
        let calls: Vec<_> = blocks
            .iter()
            .filter_map(|b| {
                if let ContentBlock::ToolUse { id, name, input } = b {
                    Some((id.clone(), name.clone(), input.clone()))
                } else {
                    None
                }
            })
            .collect();

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
                ContentBlock::ToolResult { tool_use_id: id, content, is_error }
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
