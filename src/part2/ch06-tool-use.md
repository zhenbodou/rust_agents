# 第 6 章 Tool Use：让模型调用函数

> 目标：设计一个**可扩展的 Tool 注册表**，让 LLM 能安全地调用我们暴露的 Rust 函数。这是 Agent 的"手和眼"。

## 6.1 Tool Use 的本质

上一章我们用 Tool 来强制 schema。本章我们用 Tool 达成**它真正的使命**——让模型执行副作用动作：

```text
用户："帮我看看 /tmp/a.log 里有多少行 ERROR"

模型：(思考后) 我需要读文件
  → 输出 tool_use: grep_file(path="/tmp/a.log", pattern="ERROR")
宿主：执行 grep，返回 "42 匹配"
模型：收到结果，回复用户："一共 42 行 ERROR。"
```

**宿主程序**（我们的 Rust 代码）负责执行工具。模型只负责**决定什么时候、用什么参数调用**。

## 6.2 一个好的 Tool 要满足什么

从工业实践看：

1. **单一职责**：一个 tool 只做一件事
2. **参数明确**：schema 要严格，避免"你猜我要啥"
3. **描述清晰**：`description` 是给模型看的"说明书"
4. **错误可读**：失败时给模型一段**文字**解释（不要直接 panic）
5. **幂等 or 明示副作用**：能重试的显著标记；有副作用（删除、发邮件）的要在描述里警告并经过权限系统
6. **有限输出**：超大结果要截断 + 摘要，否则炸上下文

## 6.3 Rust 中的 Tool 抽象

`examples/06-tool-use/src/tool.rs`：

```rust
use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;

#[async_trait]
pub trait Tool: Send + Sync {
    /// 工具名，必须与 schema 中的 name 一致
    fn name(&self) -> &str;

    /// 给模型看的描述
    fn description(&self) -> &str;

    /// JSON Schema（input_schema 字段内容）
    fn input_schema(&self) -> Value;

    /// 真正的执行逻辑
    async fn execute(&self, input: Value, ctx: &ToolContext) -> ToolOutput;
}

/// 调用上下文：携带权限、工作目录、session 信息等
pub struct ToolContext {
    pub cwd: std::path::PathBuf,
    pub session_id: String,
    // 后面章节会补充 permission checker, logger 等
}

#[derive(Debug)]
pub struct ToolOutput {
    pub content: String,   // 返回给模型的文本（必要时截断）
    pub is_error: bool,    // 标记失败，模型会看到并自我修正
}

impl ToolOutput {
    pub fn ok(s: impl Into<String>) -> Self { Self { content: s.into(), is_error: false } }
    pub fn err(s: impl Into<String>) -> Self { Self { content: s.into(), is_error: true } }
}
```

### 注册表

```rust
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Default)]
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    pub fn register(&mut self, tool: Arc<dyn Tool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).cloned()
    }

    /// 转换为 Anthropic API 需要的 tools 字段
    pub fn as_api_schema(&self) -> Value {
        Value::Array(self.tools.values().map(|t| {
            serde_json::json!({
                "name": t.name(),
                "description": t.description(),
                "input_schema": t.input_schema(),
            })
        }).collect())
    }
}
```

## 6.4 三个生产级示例工具

### 6.4.1 `read_file`

```rust
use crate::tool::*;
use anyhow::Context;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::io::AsyncReadExt;

pub struct ReadFileTool;

#[derive(Deserialize)]
struct ReadFileArgs {
    path: String,
    #[serde(default)]
    offset: Option<usize>,
    #[serde(default)]
    limit: Option<usize>,
}

const MAX_OUTPUT_BYTES: usize = 64 * 1024; // 防上下文爆炸

#[async_trait]
impl Tool for ReadFileTool {
    fn name(&self) -> &str { "read_file" }

    fn description(&self) -> &str {
        "Read a text file from the local filesystem. \
        Supports optional line offset/limit to read portions of large files. \
        Returns at most 64KB; further content is truncated with a notice."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["path"],
            "properties": {
                "path":  {"type": "string", "description": "Absolute or cwd-relative path"},
                "offset":{"type": "integer", "minimum": 0, "description": "Line offset (0-indexed)"},
                "limit": {"type": "integer", "minimum": 1, "description": "Max lines to read"}
            }
        })
    }

    async fn execute(&self, input: Value, ctx: &ToolContext) -> ToolOutput {
        let args: ReadFileArgs = match serde_json::from_value(input) {
            Ok(a) => a, Err(e) => return ToolOutput::err(format!("invalid args: {e}")),
        };

        let path = ctx.cwd.join(&args.path);
        let mut file = match tokio::fs::File::open(&path).await {
            Ok(f) => f, Err(e) => return ToolOutput::err(format!("open {}: {e}", path.display())),
        };

        let mut buf = Vec::new();
        if let Err(e) = file.read_to_end(&mut buf).await {
            return ToolOutput::err(format!("read: {e}"));
        }

        let text = match String::from_utf8(buf) {
            Ok(t) => t,
            Err(_) => return ToolOutput::err("file is not valid UTF-8".into()),
        };

        let lines: Vec<&str> = text.lines().collect();
        let start = args.offset.unwrap_or(0).min(lines.len());
        let end   = args.limit.map(|l| (start + l).min(lines.len())).unwrap_or(lines.len());

        let mut out = String::new();
        for (i, line) in lines[start..end].iter().enumerate() {
            out.push_str(&format!("{:6}\t{}\n", start + i + 1, line));
            if out.len() > MAX_OUTPUT_BYTES {
                out.push_str("\n… [truncated, use offset/limit to read more]");
                break;
            }
        }
        ToolOutput::ok(out)
    }
}
```

注意几个**生产级细节**：

- `MAX_OUTPUT_BYTES` 防止单次吃光上下文
- 行号前缀让模型知道"第几行"，后续 edit 工具可以精确引用
- 错误返回 `is_error: true`，模型能看到并重试而不是循环卡死

### 6.4.2 `list_dir`

```rust
pub struct ListDirTool;

#[async_trait]
impl Tool for ListDirTool {
    fn name(&self) -> &str { "list_dir" }
    fn description(&self) -> &str {
        "List files and subdirectories. Respects .gitignore by default."
    }
    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["path"],
            "properties": {
                "path": {"type": "string"},
                "max_depth": {"type": "integer", "minimum": 1, "default": 3}
            }
        })
    }
    async fn execute(&self, input: Value, ctx: &ToolContext) -> ToolOutput {
        #[derive(serde::Deserialize)]
        struct A { path: String, #[serde(default="default_depth")] max_depth: usize }
        fn default_depth() -> usize { 3 }

        let a: A = match serde_json::from_value(input) { Ok(a)=>a, Err(e)=>return ToolOutput::err(e.to_string()) };
        let root = ctx.cwd.join(&a.path);

        let walker = ignore::WalkBuilder::new(&root)
            .max_depth(Some(a.max_depth))
            .hidden(false)
            .git_ignore(true)
            .build();

        let mut out = String::new();
        let mut count = 0;
        for entry in walker.flatten() {
            out.push_str(&format!("{}\n", entry.path().strip_prefix(&root).unwrap_or(entry.path()).display()));
            count += 1;
            if count >= 2000 {
                out.push_str("… [truncated at 2000 entries]\n");
                break;
            }
        }
        ToolOutput::ok(out)
    }
}
```

（用 [`ignore`](https://crates.io/crates/ignore) crate，自动遵循 `.gitignore`——ripgrep 就是这么做的。）

### 6.4.3 `run_bash` (有副作用，后面会加权限)

```rust
pub struct RunBashTool;

#[async_trait]
impl Tool for RunBashTool {
    fn name(&self) -> &str { "run_bash" }
    fn description(&self) -> &str {
        "Execute a bash command. Returns stdout + stderr. \
        DANGEROUS: may mutate filesystem, network, or processes. \
        Prefer read-only tools when possible."
    }
    fn input_schema(&self) -> Value {
        json!({
            "type":"object","required":["command"],
            "properties":{"command":{"type":"string"},
                          "timeout_sec":{"type":"integer","default":30}}
        })
    }
    async fn execute(&self, input: Value, ctx: &ToolContext) -> ToolOutput {
        #[derive(serde::Deserialize)]
        struct A { command: String, #[serde(default="ts")] timeout_sec: u64 }
        fn ts() -> u64 { 30 }

        let a: A = match serde_json::from_value(input) { Ok(a)=>a, Err(e)=>return ToolOutput::err(e.to_string()) };

        let fut = tokio::process::Command::new("bash")
            .arg("-c").arg(&a.command)
            .current_dir(&ctx.cwd)
            .output();

        let out = match tokio::time::timeout(std::time::Duration::from_secs(a.timeout_sec), fut).await {
            Ok(Ok(o)) => o,
            Ok(Err(e)) => return ToolOutput::err(format!("spawn: {e}")),
            Err(_) => return ToolOutput::err(format!("timeout after {}s", a.timeout_sec)),
        };

        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        let body = format!("exit_code: {}\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}", out.status.code().unwrap_or(-1));
        let body = truncate_middle(&body, 32 * 1024);
        if out.status.success() { ToolOutput::ok(body) } else { ToolOutput::err(body) }
    }
}

fn truncate_middle(s: &str, max: usize) -> String {
    if s.len() <= max { return s.into() }
    let head = &s[..max/2];
    let tail = &s[s.len()-max/2..];
    format!("{head}\n… [truncated {} bytes] …\n{tail}", s.len()-max)
}
```

⚠️ 这个 tool 是不安全的，第 11 章会给它加权限系统。

## 6.5 把 Tool 接入 API 请求

```rust
use std::sync::Arc;

fn build_registry() -> ToolRegistry {
    let mut r = ToolRegistry::default();
    r.register(Arc::new(ReadFileTool));
    r.register(Arc::new(ListDirTool));
    r.register(Arc::new(RunBashTool));
    r
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    let client = AnthropicClient::from_env()?;
    let registry = build_registry();
    let ctx = ToolContext {
        cwd: std::env::current_dir()?,
        session_id: uuid::Uuid::new_v4().to_string(),
    };

    let resp = client.complete(CompleteRequest {
        model: "claude-opus-4-7".into(),
        max_tokens: 1024,
        system: Some("你是一个文件系统助手。".into()),
        messages: vec![Message::user("当前目录下有哪些 .rs 文件？给我列前 20 个。")],
        temperature: Some(0.0),
        tools: Some(registry.as_api_schema()),
    }).await?;

    // 模型此时应返回一条 tool_use 内容块
    for block in &resp.content {
        if let ContentBlock::ToolUse { id, name, input } = block {
            let tool = registry.get(name).expect("unknown tool");
            let out = tool.execute(input.clone(), &ctx).await;
            println!("tool {} => error={}, output:\n{}", name, out.is_error, out.content);
            // 下一章我们会把这个结果送回模型，完成完整循环
            let _ = id; // 下一章使用
        }
    }
    Ok(())
}
```

## 6.6 小结

- Tool 是 **trait 对象 + JSON Schema**
- 写好一个 tool = 单一职责 + 明确 schema + 有限输出 + 错误可读
- 有副作用的 tool 必须经过权限（下一步会实现）

> **下一章**：把工具结果送回模型，跑完完整的 Agent Loop。

