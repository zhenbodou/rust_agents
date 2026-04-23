//! 工具集合：read_file / list_dir / write_file / edit_file / run_bash / grep。
//! 详细注释见《实战》第 22 章。

use async_trait::async_trait;
use mcc_core::{Tool, ToolContext, ToolOutput};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

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
    pub fn subset(&self, allowed: &[String]) -> Self {
        let mut r = Self::default();
        for n in allowed {
            if let Some(t) = self.tools.get(n) {
                r.tools.insert(n.clone(), t.clone());
            }
        }
        r
    }
}

fn resolve(cwd: &Path, p: &str) -> PathBuf {
    let pb = PathBuf::from(p);
    if pb.is_absolute() { pb } else { cwd.join(pb) }
}

// ==================== read_file ====================

pub struct ReadFileTool;

#[async_trait]
impl Tool for ReadFileTool {
    fn name(&self) -> &str { "read_file" }
    fn description(&self) -> &str { "Read a UTF-8 text file with line numbers." }
    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["path"],
            "properties": {
                "path":   {"type": "string"},
                "offset": {"type": "integer", "minimum": 0},
                "limit":  {"type": "integer", "minimum": 1, "default": 2000}
            }
        })
    }
    async fn execute(&self, input: Value, ctx: &ToolContext) -> ToolOutput {
        #[derive(serde::Deserialize)]
        struct A { path: String, offset: Option<usize>, limit: Option<usize> }
        let a: A = match serde_json::from_value(input) { Ok(a) => a, Err(e) => return ToolOutput::err(e.to_string()) };

        let path = resolve(&ctx.cwd, &a.path);
        let body = match tokio::fs::read_to_string(&path).await {
            Ok(b) => b,
            Err(e) => return ToolOutput::err(format!("read {}: {e}", path.display())),
        };
        let lines: Vec<&str> = body.lines().collect();
        let total = lines.len();
        let start = a.offset.unwrap_or(0).min(total);
        let end = (start + a.limit.unwrap_or(2000)).min(total);

        let mut out = String::new();
        for (i, l) in lines[start..end].iter().enumerate() {
            out.push_str(&format!("{:>6}\t{}\n", start + i + 1, l));
        }
        if end < total {
            out.push_str(&format!("\n… [{} more lines]\n", total - end));
        }
        ToolOutput::ok(out)
    }
}

// ==================== list_dir ====================

pub struct ListDirTool;

#[async_trait]
impl Tool for ListDirTool {
    fn name(&self) -> &str { "list_dir" }
    fn description(&self) -> &str { "List directory, respects .gitignore." }
    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["path"],
            "properties": {
                "path":      {"type": "string"},
                "max_depth": {"type": "integer", "default": 3}
            }
        })
    }
    async fn execute(&self, input: Value, ctx: &ToolContext) -> ToolOutput {
        #[derive(serde::Deserialize)]
        struct A { path: String, #[serde(default = "d3")] max_depth: usize }
        fn d3() -> usize { 3 }
        let a: A = match serde_json::from_value(input) { Ok(a) => a, Err(e) => return ToolOutput::err(e.to_string()) };
        let root = resolve(&ctx.cwd, &a.path);

        let walker = ignore::WalkBuilder::new(&root)
            .max_depth(Some(a.max_depth))
            .git_ignore(true)
            .build();

        let mut out = String::new();
        let mut count = 0;
        for entry in walker.flatten() {
            let rel = entry.path().strip_prefix(&root).unwrap_or(entry.path());
            out.push_str(&format!("{}\n", rel.display()));
            count += 1;
            if count >= 2000 {
                out.push_str("… [truncated]\n");
                break;
            }
        }
        ToolOutput::ok(out)
    }
}

// ==================== write_file ====================

pub struct WriteFileTool;

#[async_trait]
impl Tool for WriteFileTool {
    fn name(&self) -> &str { "write_file" }
    fn description(&self) -> &str {
        "Create a new file. Fails if file already exists; use edit_file to modify."
    }
    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["path", "content"],
            "properties": {"path": {"type": "string"}, "content": {"type": "string"}}
        })
    }
    async fn execute(&self, input: Value, ctx: &ToolContext) -> ToolOutput {
        #[derive(serde::Deserialize)]
        struct A { path: String, content: String }
        let a: A = match serde_json::from_value(input) { Ok(a) => a, Err(e) => return ToolOutput::err(e.to_string()) };
        let path = resolve(&ctx.cwd, &a.path);
        if path.exists() {
            return ToolOutput::err(format!(
                "refusing to overwrite {}; use edit_file",
                path.display()
            ));
        }
        if let Some(p) = path.parent() {
            if let Err(e) = tokio::fs::create_dir_all(p).await {
                return ToolOutput::err(format!("mkdir: {e}"));
            }
        }
        if let Err(e) = tokio::fs::write(&path, a.content.as_bytes()).await {
            return ToolOutput::err(format!("write: {e}"));
        }
        ToolOutput::ok(format!("written: {} ({} bytes)", path.display(), a.content.len()))
    }
}

// ==================== edit_file ====================

pub struct EditFileTool;

#[async_trait]
impl Tool for EditFileTool {
    fn name(&self) -> &str { "edit_file" }
    fn description(&self) -> &str {
        "Replace exact text in file. old_string must be unique or set replace_all."
    }
    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["path", "old_string", "new_string"],
            "properties": {
                "path":        {"type": "string"},
                "old_string":  {"type": "string"},
                "new_string":  {"type": "string"},
                "replace_all": {"type": "boolean", "default": false}
            }
        })
    }
    async fn execute(&self, input: Value, ctx: &ToolContext) -> ToolOutput {
        #[derive(serde::Deserialize)]
        struct A {
            path: String,
            old_string: String,
            new_string: String,
            #[serde(default)]
            replace_all: bool,
        }
        let a: A = match serde_json::from_value(input) { Ok(a) => a, Err(e) => return ToolOutput::err(e.to_string()) };

        if a.old_string == a.new_string {
            return ToolOutput::err("old_string equals new_string");
        }
        let path = resolve(&ctx.cwd, &a.path);
        let body = match tokio::fs::read_to_string(&path).await {
            Ok(b) => b,
            Err(e) => return ToolOutput::err(format!("read: {e}")),
        };

        let count = body.matches(&a.old_string).count();
        if count == 0 {
            return ToolOutput::err("old_string not found");
        }
        if count > 1 && !a.replace_all {
            return ToolOutput::err(format!(
                "old_string matches {count} times; add context or set replace_all=true"
            ));
        }

        let new_body = if a.replace_all {
            body.replace(&a.old_string, &a.new_string)
        } else {
            body.replacen(&a.old_string, &a.new_string, 1)
        };

        let tmp = path.with_extension(format!(
            "{}.mcctmp",
            path.extension().and_then(|s| s.to_str()).unwrap_or("bak")
        ));
        if let Err(e) = tokio::fs::write(&tmp, new_body.as_bytes()).await {
            return ToolOutput::err(format!("write tmp: {e}"));
        }
        if let Err(e) = tokio::fs::rename(&tmp, &path).await {
            let _ = tokio::fs::remove_file(&tmp).await;
            return ToolOutput::err(format!("rename: {e}"));
        }
        ToolOutput::ok(format!("edited: {} ({} replacement{})", path.display(), count, if count == 1 { "" } else { "s" }))
    }
}

// ==================== default_registry ====================

pub fn default_registry() -> ToolRegistry {
    let mut r = ToolRegistry::default();
    r.register(Arc::new(ReadFileTool));
    r.register(Arc::new(ListDirTool));
    r.register(Arc::new(WriteFileTool));
    r.register(Arc::new(EditFileTool));
    r
}
