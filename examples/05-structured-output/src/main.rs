//! 第 5 章：结构化输出 —— 用 Tool Use 强制 JSON schema。

use anyhow::Result;
use ex04_llm_api::{AnthropicClient, CompleteRequest, ContentBlock, LlmProvider, Message};
use serde::Deserialize;
use serde_json::json;

#[derive(Debug, Deserialize)]
struct ReviewResult {
    issues: Vec<Issue>,
}

#[derive(Debug, Deserialize)]
struct Issue {
    severity: String,
    category: String,
    line: u32,
    message: String,
    suggestion: String,
}

fn review_tool_schema() -> serde_json::Value {
    json!({
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
                        "required": ["severity", "category", "line", "message", "suggestion"],
                        "properties": {
                            "severity":   {"enum": ["critical", "major", "minor"]},
                            "category":   {"enum": ["bug", "perf", "security", "style"]},
                            "line":       {"type": "integer"},
                            "message":    {"type": "string"},
                            "suggestion": {"type": "string"}
                        }
                    }
                }
            }
        }
    })
}

/// 备用方案：从模型的文本输出中抽出 JSON，应对未用 tool_use 的情况。
pub fn extract_json(s: &str) -> Result<String> {
    if serde_json::from_str::<serde_json::Value>(s.trim()).is_ok() {
        return Ok(s.trim().to_string());
    }
    let re = regex::Regex::new(r"(?s)```(?:json)?\s*(\{.*?\})\s*```")?;
    if let Some(cap) = re.captures(s) {
        return Ok(cap[1].to_string());
    }
    let start = s.find('{').ok_or_else(|| anyhow::anyhow!("no json"))?;
    let end = s.rfind('}').ok_or_else(|| anyhow::anyhow!("no json"))?;
    Ok(s[start..=end].to_string())
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt().with_env_filter("info").init();

    let diff = r#"--- a/src/main.rs
+++ b/src/main.rs
@@ -10,3 +10,7 @@
+fn divide(a: i32, b: i32) -> i32 {
+    a / b
+}
+
+let s: String = get_config().unwrap();
"#;

    if std::env::var("ANTHROPIC_API_KEY").is_err() {
        eprintln!("ANTHROPIC_API_KEY not set — demo mode (extract_json only):");
        let text = r#"```json
{"issues":[{"severity":"critical","category":"bug","line":3,"message":"divide by zero panic","suggestion":"return Result<i32, Err>"}]}
```"#;
        let parsed: ReviewResult = serde_json::from_str(&extract_json(text)?)?;
        println!("{parsed:#?}");
        return Ok(());
    }

    let client = AnthropicClient::from_env()?;

    let resp = client.complete(CompleteRequest {
        model: "claude-opus-4-7".into(),
        max_tokens: 1024,
        system: Some("你是严格的 Rust 代码审查员。".into()),
        messages: vec![Message::user(format!(
            "审查以下 diff：\n\n<diff>\n{diff}\n</diff>"
        ))],
        temperature: Some(0.0),
        tools: Some(json!([review_tool_schema()])),
    }).await?;

    for block in resp.content {
        if let ContentBlock::ToolUse { name, input, .. } = block {
            if name == "report_issues" {
                let parsed: ReviewResult = serde_json::from_value(input)?;
                for (i, issue) in parsed.issues.iter().enumerate() {
                    println!(
                        "#{} [{}/{}] line {}: {}\n  => {}",
                        i + 1, issue.severity, issue.category,
                        issue.line, issue.message, issue.suggestion
                    );
                }
            }
        }
    }
    Ok(())
}
