# 第 26 章 Subagent 并行执行

> 把第 14 章的 Subagent 接入 mini-claude-code，真正在大仓库里跑起来。

## 26.1 预设库：与产品捆绑分发

```rust
pub fn default_presets(cfg: &Config) -> HashMap<String, SubagentSpec> {
    let mut m = HashMap::new();

    m.insert("code_explorer".into(), SubagentSpec {
        name: "code_explorer".into(),
        system: include_str!("../presets/code_explorer.md").into(),
        model: cfg.model.subagent.clone(),
        tools: vec!["read_file".into(), "list_dir".into(), "grep".into()],
        max_iterations: 10,
        max_tokens: 2048,
        budget_tokens: 80_000,
        timeout: Duration::from_secs(120),
    });

    m.insert("test_runner".into(), SubagentSpec {
        name: "test_runner".into(),
        system: include_str!("../presets/test_runner.md").into(),
        model: cfg.model.subagent.clone(),
        tools: vec!["run_bash".into(), "read_file".into()],
        max_iterations: 10,
        max_tokens: 2048,
        budget_tokens: 150_000,
        timeout: Duration::from_secs(300),
    });

    m.insert("summarizer".into(), SubagentSpec {
        name: "summarizer".into(),
        system: "你是一个只回答摘要的子 Agent。不要使用工具，直接基于给定任务压缩输出。".into(),
        model: cfg.model.summarize.clone(),
        tools: vec![],
        max_iterations: 1,
        max_tokens: 1024,
        budget_tokens: 30_000,
        timeout: Duration::from_secs(30),
    });
    m
}
```

`presets/code_explorer.md`:

```markdown
你是代码探索员。给定任务：
1. 用 list_dir / grep 定位最多 5 个相关文件
2. 用 read_file 仔细阅读（可分页）
3. 输出结构化摘要：
   - 相关文件与行号
   - 每个文件的职责
   - 关键的类型/函数
   - 潜在问题
最终回答不超过 400 字，不要贴原代码块（除非确有必要，最多 10 行）。
```

## 26.2 SpawnSubagentTool 实现

```rust
pub struct SpawnSubagentTool {
    pub runner: Arc<SubagentRunner>,
    pub presets: Arc<HashMap<String, SubagentSpec>>,
    pub max_depth: u32,
}

#[async_trait]
impl Tool for SpawnSubagentTool {
    fn name(&self) -> &str { "spawn_subagent" }
    fn description(&self) -> &str {
        "Delegate to a specialized subagent with its own isolated context. \
        Available presets: code_explorer (read files+grep), test_runner (run tests), summarizer (pure LLM). \
        Returns only a concise summary. \
        Use when: reading many files for a small answer, OR parallelizing independent tasks."
    }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type":"object","required":["preset","task"],
            "properties":{
                "preset":{"enum":["code_explorer","test_runner","summarizer"]},
                "task":{"type":"string","description":"One-shot self-contained instruction"}
            }
        })
    }
    async fn execute(&self, input: Value, ctx: &ToolContext) -> ToolOutput {
        if ctx.depth >= self.max_depth {
            return ToolOutput::err(format!("subagent depth limit {} reached", self.max_depth));
        }
        #[derive(serde::Deserialize)] struct A { preset: String, task: String }
        let a: A = match serde_json::from_value(input) { Ok(a)=>a, Err(e)=>return ToolOutput::err(e.to_string()) };

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
```

我们给 `ToolContext` 加字段：

```rust
#[derive(Clone)]
pub struct ToolContext {
    pub cwd: PathBuf,
    pub session_id: String,
    pub depth: u32,                    // 主 Agent = 0，subagent = 1, 2...
}
```

## 26.3 并发上限

让多个 subagent 能真并行，但受总闸控：

```rust
pub struct SubagentRunner {
    llm: Arc<dyn LlmProvider>,
    registry: Arc<ToolRegistry>,
    semaphore: Arc<tokio::sync::Semaphore>,
    dispatcher: Arc<HookDispatcher>,
    recorder: Arc<SessionRecorder>,
}

impl SubagentRunner {
    pub fn new(llm: Arc<dyn LlmProvider>, registry: Arc<ToolRegistry>, concurrency: usize,
               dispatcher: Arc<HookDispatcher>, recorder: Arc<SessionRecorder>) -> Self
    {
        Self { llm, registry, semaphore: Arc::new(tokio::sync::Semaphore::new(concurrency)),
               dispatcher, recorder }
    }

    pub async fn run(&self, spec: SubagentSpec, task: String, parent: &ToolContext)
        -> anyhow::Result<SubagentResult>
    {
        let _permit = self.semaphore.acquire().await?;

        let ctx = ToolContext {
            cwd: parent.cwd.clone(),
            session_id: format!("{}::{}", parent.session_id, spec.name),
            depth: parent.depth + 1,
        };

        let agent = ProductionAgent {
            llm: self.llm.clone(),
            registry: Arc::new(self.registry.subset(&spec.tools)),
            ctx: ctx.clone(),
            system: spec.system.clone(),
            model: spec.model.clone(),
            config: AgentConfig {
                max_iterations: spec.max_iterations,
                max_tokens_per_call: spec.max_tokens,
                budget_usd: token_budget_to_usd(spec.budget_tokens, &spec.model),
                temperature: 0.0,
                retry: RetryPolicy::default(),
            },
            recorder: self.recorder.clone(),
            dispatcher: self.dispatcher.clone(),
            event_tx: noop_tx(),
            cancel: tokio_util::sync::CancellationToken::new(),
            cost: Arc::new(Mutex::new(CostTracker::default())),
        };

        let fut = agent.run(task);
        let run = tokio::time::timeout(spec.timeout, fut).await??;

        let _ = self.dispatcher.dispatch(HookEvent::SubagentStop {
            session_id: ctx.session_id.clone(),
            subagent_id: spec.name.clone(),
        }).await;

        Ok(SubagentResult {
            name: spec.name,
            summary: run.final_text,
            usage: run.total_usage,
            stopped_reason: "end_turn".into(),
        })
    }
}
```

## 26.4 示例：Slash 命令 `/analyze-repo`

`commands/analyze-repo.md`：

```markdown
---
command: /analyze-repo
description: 并行探索当前仓库的架构
---
请并行启动 4 个子 Agent 分析当前仓库：
1. spawn_subagent(code_explorer, "分析入口点（main/bin）")
2. spawn_subagent(code_explorer, "分析核心业务模块（core/domain）")
3. spawn_subagent(code_explorer, "分析 I/O 层（db/api/rpc）")
4. spawn_subagent(code_explorer, "分析测试组织")

收到 4 份摘要后，综合成一个整体架构说明：模块图、关键抽象、风险点。
```

用户在 TUI 里敲 `/analyze-repo`，主 Agent 一轮发出 4 个 `spawn_subagent` tool call，Agent Loop 的并发执行自动 fan-out。30 秒内得到整体分析——**这就是 Subagent 的实际价值**。

## 26.5 观察 Subagent

TUI 可以在消息流里渲染：

```text
▶ spawn_subagent(code_explorer, "分析入口点…")
  └─ ◎ [sub] 主入口在 crates/mcc-cli/src/main.rs，使用 clap 解析…
▶ spawn_subagent(code_explorer, "分析核心业务模块…")
  └─ ◎ [sub] core 层由 3 个 trait 支撑：Tool、Hook、LlmProvider…
```

实现：subagent 的 `event_tx` 可传入一个带前缀包装的 sender，UI 按 session_id 前缀缩进展示。

## 26.6 小结

- 预设库包含高频角色，便于主 Agent 一句话调用
- 并发 Semaphore + 超时 + depth 三重护栏
- `/analyze-repo` 是教科书级 fan-out 用例
- Subagent 的 ROI：大任务下成本降 3–5 倍（便宜模型跑脏活）

> **下一章**：打包发布，把 mcc 做成真正能 `cargo install` 的产品。

