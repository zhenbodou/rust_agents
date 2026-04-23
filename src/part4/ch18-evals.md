# 第 18 章 Evals：Agent 的测试与评估体系

> "没有 eval 的 prompt 改动，是对用户的随机游走。" —— 面试时会被问到的一句话。

## 18.1 为什么传统单测不够

Agent 的输出是**自然语言 + 多步行为**，同一个输入不一定完全相同。传统 `assert_eq` 失效。需要多种检查方式：

| 检查类型 | 适用 | 例子 |
|---|---|---|
| 精确匹配 | 结构化工具输出 | tool 调用参数是否正确 |
| Schema 校验 | JSON 输出 | 字段合规 |
| 关键词 / 正则 | 粗粒度 | "答复里包含 'unwrap'" |
| LLM-as-Judge | 质量/风格 | "回答是否礼貌且准确" |
| 端到端行为 | Agent 全流程 | "让 Agent 修 bug，修完后测试通过" |

## 18.2 Eval 数据集的结构

```yaml
- id: fix-div-by-zero
  input: "文件 src/math.rs 的 divide 函数会 panic，请修复并保证测试通过"
  fixtures:
    files:
      "src/math.rs": |
        pub fn divide(a: i32, b: i32) -> i32 { a / b }
      "tests/math.rs": |
        #[test] fn div_by_zero_returns_err() { assert!(crate::math::divide_safe(10, 0).is_err()); }
  expectations:
    - type: bash
      cmd: "cargo test"
      must_succeed: true
    - type: contains_file
      path: "src/math.rs"
      substring: "Result"
    - type: not_contains_file
      path: "src/math.rs"
      substring: "panic!"
    - type: judge
      prompt: "以下修复是否符合 Rust 习惯（优先 Result，避免 unwrap）？"
      min_score: 4
  budget:
    max_iterations: 15
    max_usd: 0.50
```

## 18.3 Runner：Rust 实现

```rust
#[derive(Debug, serde::Deserialize)]
pub struct EvalCase {
    pub id: String,
    pub input: String,
    pub fixtures: Fixtures,
    pub expectations: Vec<Expectation>,
    pub budget: Budget,
}

#[derive(Debug, serde::Deserialize)]
pub struct Fixtures {
    #[serde(default)]
    pub files: HashMap<String, String>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Expectation {
    Bash { cmd: String, must_succeed: bool },
    ContainsFile { path: String, substring: String },
    NotContainsFile { path: String, substring: String },
    Judge { prompt: String, min_score: u8 },
}

#[derive(Debug, serde::Deserialize)]
pub struct Budget { pub max_iterations: u32, pub max_usd: f64 }

pub struct EvalRunner {
    pub agent_factory: Arc<dyn Fn(&Path) -> AgentLoop + Send + Sync>,
    pub judge_llm: Arc<dyn LlmProvider>,
}

pub struct EvalResult {
    pub case_id: String,
    pub passed: bool,
    pub failures: Vec<String>,
    pub cost_usd: f64,
    pub iterations: u32,
}

impl EvalRunner {
    pub async fn run_case(&self, case: &EvalCase) -> anyhow::Result<EvalResult> {
        let tmp = tempfile::tempdir()?;
        // 1. 布置 fixture
        for (rel, content) in &case.fixtures.files {
            let full = tmp.path().join(rel);
            if let Some(dir) = full.parent() { tokio::fs::create_dir_all(dir).await?; }
            tokio::fs::write(full, content).await?;
        }

        // 2. 跑 Agent
        let agent = (self.agent_factory)(tmp.path());
        let run = agent.run(case.input.clone()).await?;

        // 3. 校验
        let mut failures = Vec::new();
        for exp in &case.expectations {
            if let Err(e) = self.check(exp, tmp.path(), &run).await {
                failures.push(e.to_string());
            }
        }

        // 4. 预算检查
        let cost = estimate_cost(&run.total_usage);
        if cost > case.budget.max_usd {
            failures.push(format!("budget exceeded: {:.3} > {:.3}", cost, case.budget.max_usd));
        }
        if run.iterations > case.budget.max_iterations {
            failures.push(format!("iterations exceeded: {} > {}", run.iterations, case.budget.max_iterations));
        }

        Ok(EvalResult {
            case_id: case.id.clone(),
            passed: failures.is_empty(),
            failures,
            cost_usd: cost,
            iterations: run.iterations,
        })
    }

    async fn check(&self, exp: &Expectation, cwd: &Path, run: &AgentRun) -> anyhow::Result<()> {
        match exp {
            Expectation::Bash { cmd, must_succeed } => {
                let out = tokio::process::Command::new("bash").arg("-c").arg(cmd).current_dir(cwd).output().await?;
                if *must_succeed && !out.status.success() {
                    anyhow::bail!("bash failed: {}\n{}", cmd, String::from_utf8_lossy(&out.stderr));
                }
                Ok(())
            }
            Expectation::ContainsFile { path, substring } => {
                let body = tokio::fs::read_to_string(cwd.join(path)).await?;
                if !body.contains(substring) { anyhow::bail!("{} missing substring `{}`", path, substring); }
                Ok(())
            }
            Expectation::NotContainsFile { path, substring } => {
                let body = tokio::fs::read_to_string(cwd.join(path)).await.unwrap_or_default();
                if body.contains(substring) { anyhow::bail!("{} still contains `{}`", path, substring); }
                Ok(())
            }
            Expectation::Judge { prompt, min_score } => {
                let score = self.llm_judge(prompt, &run.final_text).await?;
                if score < *min_score { anyhow::bail!("judge score {} < {}", score, min_score); }
                Ok(())
            }
        }
    }

    async fn llm_judge(&self, rubric: &str, answer: &str) -> anyhow::Result<u8> {
        let resp = self.judge_llm.complete(CompleteRequest {
            model: "claude-sonnet-4-6".into(),
            max_tokens: 64,
            messages: vec![Message::user(format!(
                "Rubric:\n{rubric}\n\nCandidate answer:\n<answer>\n{answer}\n</answer>\n\n\
                 Return a single integer 1-5 where 5 is perfect. Output ONLY the number."
            ))],
            system: Some("You are a strict eval judge.".into()),
            temperature: Some(0.0),
            tools: None,
        }).await?;
        let text = resp.content.iter().filter_map(|b| if let ContentBlock::Text { text, .. } = b { Some(text.as_str()) } else { None }).next().unwrap_or("0");
        Ok(text.trim().chars().next().and_then(|c| c.to_digit(10)).unwrap_or(0) as u8)
    }
}
```

## 18.4 并行 & 报告

```rust
pub async fn run_suite(runner: &EvalRunner, cases: Vec<EvalCase>) -> Vec<EvalResult> {
    let sem = Arc::new(tokio::sync::Semaphore::new(4));  // 最多 4 并行
    let mut futs = Vec::new();
    for case in cases {
        let sem = sem.clone();
        let runner = runner.clone();  // 需要实现 Clone 或包 Arc
        futs.push(tokio::spawn(async move {
            let _permit = sem.acquire_owned().await.unwrap();
            runner.run_case(&case).await.unwrap_or_else(|e| EvalResult {
                case_id: case.id, passed: false,
                failures: vec![format!("runtime error: {e}")],
                cost_usd: 0.0, iterations: 0,
            })
        }));
    }
    let mut results = Vec::new();
    for f in futs { if let Ok(r) = f.await { results.push(r) } }
    results
}

pub fn print_report(results: &[EvalResult]) {
    let pass = results.iter().filter(|r| r.passed).count();
    println!("===== EVAL REPORT =====");
    println!("passed: {}/{}", pass, results.len());
    for r in results {
        let status = if r.passed { "PASS" } else { "FAIL" };
        println!("[{}] {}  (${:.3}, {} iters)", status, r.case_id, r.cost_usd, r.iterations);
        for f in &r.failures { println!("    - {f}"); }
    }
}
```

## 18.5 Regression / CI 集成

```yaml
# .github/workflows/evals.yml
name: evals
on:
  pull_request:
    paths: ["prompts/**", "src/**", "evals/**"]
jobs:
  eval:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo build --release
      - run: ./target/release/eval-runner --suite evals/regression.yaml --threshold 0.9
        env:
          ANTHROPIC_API_KEY: ${{ secrets.ANTHROPIC_API_KEY }}
```

**门禁**：通过率 < 90% 阻止合并。

## 18.6 三类必备 eval 集

1. **Unit Evals**：检查单个 skill / tool 的行为
2. **End-to-End Evals**：用真实仓库、真实任务（`swe-bench`、`ctf`、自建业务任务）
3. **Red Team Evals**：Prompt injection、越权、危险 tool 调用——检查有没有被攻破

## 18.7 小结

- 四种检查手段组合拳
- YAML 驱动 + Rust runner + 并行
- CI 门禁是 eval 真正发挥价值的地方
- 永远先写 eval 再改 prompt（就像先写测试再改代码）

> **下一章**：安全——Prompt Injection 与数据泄露防御。

