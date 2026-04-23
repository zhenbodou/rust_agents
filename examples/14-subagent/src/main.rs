//! 第 14 章：Subagent —— 让主 Agent 派小弟干活。

use anyhow::Result;
use async_trait::async_trait;
use ex04_llm_api::{AnthropicClient, LlmProvider, Usage};
use ex06_tool_use::{
    default_registry, Tool, ToolContext, ToolOutput, ToolRegistry,
};
use ex07_agent_loop::AgentLoop;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone)]
pub struct SubagentSpec {
    pub name: String,
    pub system: String,
    pub model: String,
    pub tools: Vec<String>,
    pub max_iterations: u32,
    pub max_tokens: u32,
    pub timeout: Duration,
}

pub struct SubagentResult {
    pub name: String,
    pub summary: String,
    pub usage: Usage,
}

pub struct SubagentRunner {
    pub llm: Arc<dyn LlmProvider>,
    pub registry: Arc<ToolRegistry>,
    pub semaphore: Arc<tokio::sync::Semaphore>,
}

impl SubagentRunner {
    pub async fn run(
        &self,
        spec: SubagentSpec,
        task: String,
        parent: &ToolContext,
    ) -> Result<SubagentResult> {
        let _permit = self.semaphore.acquire().await?;

        let ctx = ToolContext {
            cwd: parent.cwd.clone(),
            session_id: format!("{}::{}", parent.session_id, spec.name),
            depth: parent.depth + 1,
        };

        let agent = AgentLoop {
            llm: self.llm.clone(),
            registry: Arc::new(self.registry.subset(&spec.tools)),
            ctx,
            system: spec.system.clone(),
            model: spec.model.clone(),
            max_tokens: spec.max_tokens,
            max_iterations: spec.max_iterations,
            temperature: 0.0,
        };

        let run = tokio::time::timeout(spec.timeout, agent.run(task)).await??;

        Ok(SubagentResult {
            name: spec.name,
            summary: run.final_text,
            usage: run.total_usage,
        })
    }
}

pub fn default_presets() -> HashMap<String, SubagentSpec> {
    let mut m = HashMap::new();
    m.insert(
        "code_explorer".into(),
        SubagentSpec {
            name: "code_explorer".into(),
            system: "你是代码探索员。先 list_dir 和 read_file 定位最多 5 个相关文件，然后输出不超过 400 字的结构化摘要：相关文件、职责、关键类型、潜在问题。"
                .into(),
            model: "claude-haiku-4-5-20251001".into(),
            tools: vec!["read_file".into(), "list_dir".into()],
            max_iterations: 8,
            max_tokens: 2048,
            timeout: Duration::from_secs(120),
        },
    );
    m
}

pub struct SpawnSubagentTool {
    pub runner: Arc<SubagentRunner>,
    pub presets: Arc<HashMap<String, SubagentSpec>>,
    pub max_depth: u32,
}

#[async_trait]
impl Tool for SpawnSubagentTool {
    fn name(&self) -> &str { "spawn_subagent" }
    fn description(&self) -> &str {
        "Delegate to a specialized subagent with its own isolated context. Returns only a concise summary."
    }
    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["preset", "task"],
            "properties": {
                "preset": {"type": "string"},
                "task":   {"type": "string"}
            }
        })
    }
    async fn execute(&self, input: serde_json::Value, ctx: &ToolContext) -> ToolOutput {
        if ctx.depth >= self.max_depth {
            return ToolOutput::err(format!("subagent depth limit {} reached", self.max_depth));
        }
        #[derive(serde::Deserialize)]
        struct A { preset: String, task: String }
        let a: A = match serde_json::from_value(input) {
            Ok(a) => a, Err(e) => return ToolOutput::err(e.to_string()),
        };
        let spec = match self.presets.get(&a.preset) {
            Some(s) => s.clone(),
            None => return ToolOutput::err(format!("unknown preset: {}", a.preset)),
        };
        match self.runner.run(spec, a.task, ctx).await {
            Ok(r) => ToolOutput::ok(format!(
                "[subagent={} tokens={}in/{}out]\n{}",
                r.name, r.usage.input_tokens, r.usage.output_tokens, r.summary
            )),
            Err(e) => ToolOutput::err(e.to_string()),
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt().with_env_filter("info").init();

    if std::env::var("ANTHROPIC_API_KEY").is_err() {
        eprintln!("ANTHROPIC_API_KEY not set — skipping live demo.");
        println!("Registered presets: {:?}", default_presets().keys().collect::<Vec<_>>());
        return Ok(());
    }

    let llm = Arc::new(AnthropicClient::from_env()?);
    let registry = Arc::new(default_registry());
    let runner = Arc::new(SubagentRunner {
        llm,
        registry,
        semaphore: Arc::new(tokio::sync::Semaphore::new(4)),
    });

    let spec = default_presets().remove("code_explorer").unwrap();
    let ctx = ToolContext {
        cwd: std::env::current_dir()?,
        session_id: "demo".into(),
        depth: 0,
    };
    let r = runner.run(spec, "分析当前目录的项目结构".into(), &ctx).await?;
    println!("=== subagent result ===\n{}\n\nusage: {:?}", r.summary, r.usage);
    Ok(())
}
