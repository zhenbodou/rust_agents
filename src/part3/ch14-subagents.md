# 第 14 章 Subagents 与任务分解

> 让一个 Agent 同时 "读 100 个文件找 bug" 会把上下文炸穿。答案是：**派小弟**。

## 14.1 为什么需要 Subagent

三个核心原因：

1. **上下文隔离**：子 Agent 在自己的上下文里啃大量原始资料，只把**结论**返还给主 Agent
2. **并行加速**：多个子 Agent 同时工作
3. **角色分工**：子 Agent 可以用不同 system prompt / 不同模型（便宜的 Haiku 做搜索，贵的 Opus 做决策）

Claude Code 的 `Task` / `Agent` 工具、Cursor 的 "Compose agents" 都是这个思路。

## 14.2 架构

```text
       ┌────────────┐
       │  主 Agent  │  (Opus，规划 + 整合)
       │  上下文:   │
       │  - 用户    │
       │  - TODO    │
       └─────┬──────┘
             │ spawn
   ┌─────────┼─────────┐
   ▼         ▼         ▼
┌──────┐ ┌──────┐ ┌──────┐
│Sub A │ │Sub B │ │Sub C │  (Haiku/Sonnet，各自独立上下文)
│读文件│ │跑测试│ │查网络│
└──┬───┘ └──┬───┘ └──┬───┘
   │        │        │
   └────────┼────────┘
            ▼
      只返回"结论" (100–500 tokens)
            │
            ▼
       主 Agent 继续
```

**关键**：子 Agent 的上下文**不回传**给主 Agent，只回传精炼结论。

## 14.3 Rust 实现

`examples/14-subagent` 的核心 API：

```rust
pub struct SubagentSpec {
    pub name: String,
    pub system: String,           // 自己的人格
    pub model: String,            // 可用更便宜的
    pub tools: Vec<String>,       // 从主 Agent 的工具中筛选可用子集
    pub max_iterations: u32,
    pub max_tokens: u32,
    pub budget_tokens: u32,       // 总预算（input+output），超了终止
    pub timeout: std::time::Duration,
}

pub struct SubagentResult {
    pub name: String,
    pub summary: String,
    pub usage: Usage,
    pub stopped_reason: String,
}

pub struct SubagentRunner {
    llm: Arc<dyn LlmProvider>,
    registry: Arc<ToolRegistry>,
}

impl SubagentRunner {
    pub async fn run(&self, spec: SubagentSpec, task: String, parent_ctx: &ToolContext)
        -> anyhow::Result<SubagentResult>
    {
        let registry_subset = self.registry.subset(&spec.tools);
        let ctx = ToolContext {
            cwd: parent_ctx.cwd.clone(),
            session_id: format!("{}::sub::{}", parent_ctx.session_id, spec.name),
        };

        let agent = AgentLoop {
            llm: self.llm.clone(),
            registry: Arc::new(registry_subset),
            ctx,
            system: spec.system.clone(),
            model: spec.model.clone(),
            max_tokens: spec.max_tokens,
            max_iterations: spec.max_iterations,
            temperature: 0.0,
        };

        let started = std::time::Instant::now();
        let fut = agent.run(task);
        let run = tokio::time::timeout(spec.timeout, fut).await??;

        Ok(SubagentResult {
            name: spec.name,
            summary: run.final_text,
            usage: run.total_usage,
            stopped_reason: if started.elapsed() > spec.timeout { "timeout".into() } else { "end_turn".into() },
        })
    }
}
```

`ToolRegistry::subset`：

```rust
impl ToolRegistry {
    pub fn subset(&self, allowed: &[String]) -> ToolRegistry {
        let mut r = ToolRegistry::default();
        for n in allowed {
            if let Some(t) = self.tools.get(n) { r.tools.insert(n.clone(), t.clone()); }
        }
        r
    }
}
```

## 14.4 把 "spawn subagent" 做成工具

主 Agent 通过工具调用发起子 Agent：

```rust
pub struct SpawnSubagentTool {
    runner: Arc<SubagentRunner>,
    presets: HashMap<String, SubagentSpec>,
}

#[async_trait]
impl Tool for SpawnSubagentTool {
    fn name(&self) -> &str { "spawn_subagent" }
    fn description(&self) -> &str {
        "Delegate a focused task to a specialized subagent with its own context. \
        Use when: (a) task needs reading many files only to extract a small answer, \
        (b) multiple independent sub-tasks can run in parallel. \
        Subagent returns a concise summary only (its raw context is NOT shared back)."
    }
    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["preset", "task"],
            "properties": {
                "preset": {"type": "string", "description": "Subagent persona id"},
                "task":   {"type": "string", "description": "One-shot instruction"}
            }
        })
    }

    async fn execute(&self, input: serde_json::Value, ctx: &ToolContext) -> ToolOutput {
        #[derive(serde::Deserialize)] struct A { preset: String, task: String }
        let a: A = match serde_json::from_value(input) { Ok(v)=>v, Err(e)=>return ToolOutput::err(e.to_string()) };

        let spec = match self.presets.get(&a.preset).cloned() {
            Some(s) => s,
            None => return ToolOutput::err(format!("unknown preset: {}", a.preset)),
        };

        match self.runner.run(spec, a.task, ctx).await {
            Ok(r) => ToolOutput::ok(format!(
                "subagent={}\ntokens={}in/{}out\n---\n{}",
                r.name, r.usage.input_tokens, r.usage.output_tokens, r.summary
            )),
            Err(e) => ToolOutput::err(e.to_string()),
        }
    }
}
```

## 14.5 并行 Fan-out

主 Agent 一次调用要启动 N 个子 Agent？让 `spawn_subagent` 支持数组，或者主 Agent 单轮多次调用（第 7 章的 loop 已经会并行执行）：

```rust
// 主 Agent 在一轮可能产生：
// tool_use: spawn_subagent(preset="file_reader", task="Summarize src/auth.rs")
// tool_use: spawn_subagent(preset="file_reader", task="Summarize src/db.rs")
// tool_use: spawn_subagent(preset="file_reader", task="Summarize src/api.rs")
//
// AgentLoop::run_tool_calls 会用 tokio::spawn 并行执行三个
```

三个子 Agent 同时跑，每个都在自己的上下文里读完文件，只返回摘要。相当于把"大仓库理解"并行化。

## 14.6 预设库：一组常用 Subagent

```rust
fn default_presets() -> HashMap<String, SubagentSpec> {
    let mut m = HashMap::new();
    m.insert("file_reader".into(), SubagentSpec {
        name: "file_reader".into(),
        system: "你是一个文件总结员。读取给定文件或目录，输出结构化摘要：模块职责、核心类型、导出 API、依赖。回答不超过 500 字。".into(),
        model: "claude-haiku-4-5-20251001".into(),     // 便宜快
        tools: vec!["read_file".into(), "list_dir".into(), "grep".into()],
        max_iterations: 6,
        max_tokens: 1024,
        budget_tokens: 50_000,
        timeout: std::time::Duration::from_secs(90),
    });

    m.insert("test_runner".into(), SubagentSpec {
        name: "test_runner".into(),
        system: "你运行测试并汇总失败：测试名、失败断言、最可能原因。".into(),
        model: "claude-sonnet-4-6".into(),
        tools: vec!["run_bash".into(), "read_file".into()],
        max_iterations: 10,
        max_tokens: 2048,
        budget_tokens: 200_000,
        timeout: std::time::Duration::from_secs(300),
    });

    m.insert("web_researcher".into(), SubagentSpec {
        name: "web_researcher".into(),
        system: "你做网络调研，返回带出处的简短答案。".into(),
        model: "claude-sonnet-4-6".into(),
        tools: vec!["web_search".into(), "fetch_url".into()],
        max_iterations: 8,
        max_tokens: 2048,
        budget_tokens: 150_000,
        timeout: std::time::Duration::from_secs(120),
    });
    m
}
```

## 14.7 陷阱

### 14.7.1 "递归地狱"

子 Agent 如果也能 spawn subagent，可能栈溢出。对策：

- 给 `ToolContext` 加 `depth: u32`，超过 3 禁止继续 spawn
- 主 Agent 的 `spawn_subagent` 不给子 Agent 用

### 14.7.2 成本失控

子 Agent 跑起来就是花钱。必加：

- `budget_tokens`：累加 input+output，超了立刻 stop
- 全局 circuit breaker（session 总预算）

### 14.7.3 结论质量

子 Agent 的结论如果不准，主 Agent 会基于错误结论继续。对策：

- 子 Agent system 里要求 "如果不确定，明确说 'I am not confident about ...'"
- 关键决策加 verifier 子 Agent（让另一个 Agent 审第一个的结论）

## 14.8 小结

- Subagent = 独立上下文 + 子任务 + 精炼回答
- Rust 实现就是递归复用 `AgentLoop` + 筛选工具 + 超时与预算
- 必须有 depth、budget、timeout 三重护栏

> 🎉 **Part 3 完结**。你现在已掌握完整的 Harness Engineering 工具箱：权限、Hooks、Skills、Subagents。下一部分我们进入生产级工程。

