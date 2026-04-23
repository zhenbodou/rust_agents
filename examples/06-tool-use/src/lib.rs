//! 第 6 章：Tool trait + Registry + 生产级示例工具。

use async_trait::async_trait;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn input_schema(&self) -> Value;
    async fn execute(&self, input: Value, ctx: &ToolContext) -> ToolOutput;
}

#[derive(Clone)]
pub struct ToolContext {
    pub cwd: PathBuf,
    pub session_id: String,
    pub depth: u32,
}

#[derive(Debug, Clone)]
pub struct ToolOutput {
    pub content: String,
    pub is_error: bool,
}

impl ToolOutput {
    pub fn ok(s: impl Into<String>) -> Self {
        Self { content: s.into(), is_error: false }
    }
    pub fn err(s: impl Into<String>) -> Self {
        Self { content: s.into(), is_error: true }
    }
}

#[derive(Default, Clone)]
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

    pub fn as_api_schema(&self) -> Value {
        Value::Array(
            self.tools
                .values()
                .map(|t| {
                    json!({
                        "name": t.name(),
                        "description": t.description(),
                        "input_schema": t.input_schema(),
                    })
                })
                .collect(),
        )
    }

    pub fn subset(&self, allowed: &[String]) -> ToolRegistry {
        let mut r = ToolRegistry::default();
        for n in allowed {
            if let Some(t) = self.tools.get(n) {
                r.tools.insert(n.clone(), t.clone());
            }
        }
        r
    }
}

// ============================================================================
//                              内置工具
// ============================================================================

const MAX_OUTPUT_BYTES: usize = 64 * 1024;

pub struct ReadFileTool;

#[async_trait]
impl Tool for ReadFileTool {
    fn name(&self) -> &str { "read_file" }
    fn description(&self) -> &str {
        "Read a UTF-8 text file with line numbers. Supports offset/limit for large files."
    }
    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["path"],
            "properties": {
                "path":   {"type": "string"},
                "offset": {"type": "integer", "minimum": 0},
                "limit":  {"type": "integer", "minimum": 1}
            }
        })
    }
    async fn execute(&self, input: Value, ctx: &ToolContext) -> ToolOutput {
        #[derive(serde::Deserialize)]
        struct A {
            path: String,
            offset: Option<usize>,
            limit: Option<usize>,
        }
        let a: A = match serde_json::from_value(input) {
            Ok(a) => a,
            Err(e) => return ToolOutput::err(format!("invalid args: {e}")),
        };

        let path = resolve(&ctx.cwd, &a.path);
        let body = match tokio::fs::read_to_string(&path).await {
            Ok(b) => b,
            Err(e) => return ToolOutput::err(format!("open {}: {e}", path.display())),
        };

        let lines: Vec<&str> = body.lines().collect();
        let start = a.offset.unwrap_or(0).min(lines.len());
        let end = a
            .limit
            .map(|l| (start + l).min(lines.len()))
            .unwrap_or(lines.len());

        let mut out = String::new();
        for (i, line) in lines[start..end].iter().enumerate() {
            out.push_str(&format!("{:>6}\t{}\n", start + i + 1, line));
            if out.len() > MAX_OUTPUT_BYTES {
                out.push_str("\n… [truncated]\n");
                break;
            }
        }
        ToolOutput::ok(out)
    }
}

pub struct ListDirTool;

#[async_trait]
impl Tool for ListDirTool {
    fn name(&self) -> &str { "list_dir" }
    fn description(&self) -> &str {
        "List files and subdirectories. Respects .gitignore."
    }
    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["path"],
            "properties": {
                "path":      {"type": "string"},
                "max_depth": {"type": "integer", "minimum": 1, "default": 3}
            }
        })
    }
    async fn execute(&self, input: Value, ctx: &ToolContext) -> ToolOutput {
        #[derive(serde::Deserialize)]
        struct A {
            path: String,
            #[serde(default = "default_depth")]
            max_depth: usize,
        }
        fn default_depth() -> usize { 3 }

        let a: A = match serde_json::from_value(input) {
            Ok(a) => a,
            Err(e) => return ToolOutput::err(e.to_string()),
        };
        let root = resolve(&ctx.cwd, &a.path);

        let walker = ignore::WalkBuilder::new(&root)
            .max_depth(Some(a.max_depth))
            .hidden(false)
            .git_ignore(true)
            .build();

        let mut out = String::new();
        let mut count = 0;
        for entry in walker.flatten() {
            let rel = entry
                .path()
                .strip_prefix(&root)
                .unwrap_or(entry.path());
            out.push_str(&format!("{}\n", rel.display()));
            count += 1;
            if count >= 2000 {
                out.push_str("… [truncated at 2000 entries]\n");
                break;
            }
        }
        ToolOutput::ok(out)
    }
}

pub struct RunBashTool;

#[async_trait]
impl Tool for RunBashTool {
    fn name(&self) -> &str { "run_bash" }
    fn description(&self) -> &str {
        "Execute a bash command. Returns stdout + stderr. Prefer read-only when possible."
    }
    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["command"],
            "properties": {
                "command":     {"type": "string"},
                "timeout_sec": {"type": "integer", "default": 30}
            }
        })
    }
    async fn execute(&self, input: Value, ctx: &ToolContext) -> ToolOutput {
        #[derive(serde::Deserialize)]
        struct A {
            command: String,
            #[serde(default = "t30")]
            timeout_sec: u64,
        }
        fn t30() -> u64 { 30 }

        let a: A = match serde_json::from_value(input) {
            Ok(a) => a,
            Err(e) => return ToolOutput::err(e.to_string()),
        };

        let fut = tokio::process::Command::new("bash")
            .arg("-c")
            .arg(&a.command)
            .current_dir(&ctx.cwd)
            .kill_on_drop(true)
            .output();

        let out = match tokio::time::timeout(
            std::time::Duration::from_secs(a.timeout_sec),
            fut,
        )
        .await
        {
            Ok(Ok(o)) => o,
            Ok(Err(e)) => return ToolOutput::err(format!("spawn: {e}")),
            Err(_) => return ToolOutput::err(format!("timeout after {}s", a.timeout_sec)),
        };

        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        let body = format!(
            "exit_code: {}\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}",
            out.status.code().unwrap_or(-1)
        );
        let body = truncate_middle(&body, 32 * 1024);
        if out.status.success() {
            ToolOutput::ok(body)
        } else {
            ToolOutput::err(body)
        }
    }
}

fn resolve(cwd: &std::path::Path, p: &str) -> PathBuf {
    let pb = PathBuf::from(p);
    if pb.is_absolute() { pb } else { cwd.join(pb) }
}

fn truncate_middle(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.into();
    }
    let head = &s[..max / 2];
    let tail = &s[s.len() - max / 2..];
    format!("{head}\n… [truncated {} bytes] …\n{tail}", s.len() - max)
}

/// 构建默认工具注册表（仅 read-only 工具 + bash）。
pub fn default_registry() -> ToolRegistry {
    let mut r = ToolRegistry::default();
    r.register(Arc::new(ReadFileTool));
    r.register(Arc::new(ListDirTool));
    r.register(Arc::new(RunBashTool));
    r
}
