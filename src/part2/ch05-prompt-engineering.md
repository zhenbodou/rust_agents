# 第 5 章 Prompt 工程与结构化输出

> 目标：让你从"祈祷模型听话"升级到"稳定可预测的结构化产出"，这是企业级 Agent 的基石。

## 5.1 Prompt 的四段结构（黄金模板）

大量工业实践总结出 Agent 场景的最佳 Prompt 结构：

```text
[角色]     你是谁，擅长什么
[上下文]   任务背景、约束、用户是谁
[任务]     现在要做什么（明确、单一目标）
[格式]     输出规范（JSON schema、字段、风格）
```

示例（一个代码审查 Agent 的 system prompt）：

```text
你是一名资深 Rust 代码审查员，在一家强调内存安全与性能的初创公司工作。

上下文：
- 审查对象是 PR diff，来自 tokio 异步服务
- 团队风格：禁用 `unwrap()`、强制 `#![deny(unsafe_code)]`、优先 `thiserror`
- 读者是 PR 作者（可能是新人）

任务：
读取 diff，指出 bug、性能、安全、风格四类问题，最多 5 条，按严重性排序。

输出格式：严格遵循以下 JSON schema（不允许多余文字）：
{
  "issues": [
    { "severity": "critical|major|minor",
      "category": "bug|perf|security|style",
      "line": <int>,
      "message": "<中文简述>",
      "suggestion": "<修复建议>" }
  ]
}
```

## 5.2 Prompt 工程 10 条实战原则

1. **具体 > 抽象**："写得简洁" vs "每个函数不超过 40 行"
2. **给例子（Few-shot）**：1–3 个高质量例子往往胜过 10 段描述
3. **角色 + 读者**：说明"你是谁"和"输出给谁看"会改变风格
4. **分步思考**：在复杂任务里加 "think step by step before answering"
5. **负面清单**：明确 "不要 xxx" 比 "要 xxx" 更有效
6. **XML 标签划分段**：Claude 对 `<context>...</context>` 这种结构特别敏感
7. **变量在后**：prompt 中变量部分放最后，让缓存前缀命中（第 16 章）
8. **输出放在回合开头**：让模型先 "prefill" 一部分结构化开头
9. **拒答要给台阶**：明确允许 "如果无法确定，回复 UNKNOWN"
10. **测试**：Prompt 要有 evals（第 18 章），不要凭感觉

## 5.3 结构化输出三种技术

### 方式一：Prompt 约束 + `serde_json` 解析

最通用。你在 system 里写死 JSON schema，然后代码里：

```rust
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct ReviewResult {
    issues: Vec<Issue>,
}

#[derive(Debug, Deserialize)]
struct Issue {
    severity: Severity,
    category: String,
    line: u32,
    message: String,
    suggestion: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Severity { Critical, Major, Minor }

fn parse_review(text: &str) -> anyhow::Result<ReviewResult> {
    // 小心：模型可能在 JSON 前后带空白或 markdown fence
    let cleaned = extract_json(text)?;
    Ok(serde_json::from_str(&cleaned)?)
}

fn extract_json(s: &str) -> anyhow::Result<String> {
    // 情形 1：裸 JSON
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(s.trim()) {
        return Ok(v.to_string());
    }
    // 情形 2：被 ```json ... ``` 包裹
    let re = regex::Regex::new(r"(?s)```(?:json)?\s*(\{.*?\})\s*```")?;
    if let Some(cap) = re.captures(s) {
        return Ok(cap[1].to_string());
    }
    // 情形 3：首个 { 到最后一个 }
    let start = s.find('{').ok_or_else(|| anyhow::anyhow!("no json"))?;
    let end = s.rfind('}').ok_or_else(|| anyhow::anyhow!("no json"))?;
    Ok(s[start..=end].to_string())
}
```

### 方式二：Tool Use 强制 schema（推荐）

把"返回结构"定义成一个**工具**，让模型必须"调用"它。这是目前最稳的方式（Anthropic 官方推荐）。

```rust
let tool = serde_json::json!({
    "name": "report_issues",
    "description": "Submit code review issues.",
    "input_schema": {
        "type": "object",
        "required": ["issues"],
        "properties": {
            "issues": {
                "type": "array",
                "items": {
                    "type": "object",
                    "required": ["severity","category","line","message","suggestion"],
                    "properties": {
                        "severity": {"enum": ["critical","major","minor"]},
                        "category": {"enum": ["bug","perf","security","style"]},
                        "line":     {"type": "integer"},
                        "message":  {"type": "string"},
                        "suggestion":{"type":"string"}
                    }
                }
            }
        }
    }
});
```

然后在请求里加 `"tool_choice": {"type": "tool", "name": "report_issues"}`，模型**必定**会产出合法 JSON（API 层校验 schema）。

### 方式三：Prefill 前缀

Anthropic API 允许你给 assistant 的第一个 token 指定开头。预填 `{` 会极大提高 JSON 稳定度：

```json
"messages": [
  {"role": "user", "content": "..."},
  {"role": "assistant", "content": "{"}
]
```

Rust 调用时就在最后 append 一条 assistant 消息开头即可。

## 5.4 完整示例：`examples/05-structured-output`

`src/main.rs` 关键片段：

```rust
use anyhow::Result;
use serde::Deserialize;
use serde_json::json;

#[derive(Debug, Deserialize)]
struct ReviewResult { issues: Vec<Issue> }

#[derive(Debug, Deserialize)]
struct Issue {
    severity: String,
    category: String,
    line: u32,
    message: String,
    suggestion: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    let client = ex04_llm_api::AnthropicClient::from_env()?;

    let diff = r#"
--- a/src/main.rs
+++ b/src/main.rs
@@ -10,3 +10,7 @@
+fn divide(a: i32, b: i32) -> i32 {
+    a / b  // 没处理 b == 0
+}
+
+let s: String = get_config().unwrap();  // unwrap on config read
"#;

    let tool = json!({
        "name": "report_issues",
        "description": "Submit code review issues.",
        "input_schema": { /* 同上，略 */ }
    });

    let resp = client.complete(ex04_llm_api::CompleteRequest {
        model: "claude-opus-4-7".into(),
        max_tokens: 1024,
        system: Some("你是严格的 Rust 代码审查员。".into()),
        messages: vec![ex04_llm_api::Message::user(format!(
            "审查以下 diff：\n\n<diff>\n{diff}\n</diff>"
        ))],
        temperature: Some(0.0),
        tools: Some(json!([tool])),
    }).await?;

    for block in resp.content {
        if let ex04_llm_api::ContentBlock::ToolUse { name, input, .. } = block {
            if name == "report_issues" {
                let parsed: ReviewResult = serde_json::from_value(input)?;
                for (i, issue) in parsed.issues.iter().enumerate() {
                    println!("#{} [{}/{}] line {}: {}\n  => {}",
                        i+1, issue.severity, issue.category, issue.line,
                        issue.message, issue.suggestion);
                }
            }
        }
    }
    Ok(())
}
```

## 5.5 Prompt 版本管理（工业实践）

生产环境里 prompt 会频繁迭代，必须像代码一样管理：

- **单文件 per prompt**：`prompts/code_review.md`
- **模板引擎**：用 [`tera`](https://crates.io/crates/tera) 或 [`handlebars`](https://crates.io/crates/handlebars) 渲染变量
- **版本号 + evals**：每改一次 prompt，跑一轮评估集（第 18 章）
- **A/B 运行**：生产流量分 5% 给新 prompt，观察指标

Rust 里用 `include_str!` 把 prompt 编译进二进制：

```rust
const SYSTEM_REVIEW: &str = include_str!("../prompts/code_review.md");
```

改 prompt 就要重编，天然触发 CI evals。

## 5.6 小结

- Prompt = 角色 + 上下文 + 任务 + 格式
- 结构化输出首选 **Tool Use 强制 schema**
- Prompt 是代码的一部分：版本化 + 有测试

> **下一章**：让模型真正调用我们的 Rust 函数——Tool Use。

