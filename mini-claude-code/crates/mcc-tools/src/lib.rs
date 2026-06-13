//! 工具集合：read_file / list_dir / write_file / edit_file / run_bash / glob / grep。
//! 详细注释见《实战》第 22 章。
//!
//! 企业级改造要点：
//!   - RunBashTool：tokio::process + 超时隔离，防止 shell 占用主线程
//!   - GrepTool：ignore crate 遵守 .gitignore，regex crate 支持 PCRE-like 语法
//!   - 所有工具均不 panic，所有路径错误直接返回 ToolOutput::err

use async_trait::async_trait;
use mcc_core::{Tool, ToolContext, ToolOutput};
use regex::Regex;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

// ──────────────────────────────────────────────────────────────────────────────
// ToolRegistry
// ──────────────────────────────────────────────────────────────────────────────

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

    pub fn names(&self) -> Vec<&str> {
        self.tools.keys().map(|s| s.as_str()).collect()
    }

    pub fn as_api_schema(&self) -> Value {
        let mut tools: Vec<_> = self
            .tools
            .values()
            .map(|t| {
                json!({
                    "name": t.name(),
                    "description": t.description(),
                    "input_schema": t.input_schema(),
                })
            })
            .collect();
        // stable ordering
        tools.sort_by(|a, b| {
            a["name"].as_str().unwrap_or("").cmp(b["name"].as_str().unwrap_or(""))
        });
        Value::Array(tools)
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

// ──────────────────────────────────────────────────────────────────────────────
// Helpers
// ──────────────────────────────────────────────────────────────────────────────

fn resolve(cwd: &Path, p: &str) -> PathBuf {
    let pb = PathBuf::from(p);
    if pb.is_absolute() { pb } else { cwd.join(pb) }
}

// ──────────────────────────────────────────────────────────────────────────────
// read_file
// ──────────────────────────────────────────────────────────────────────────────

pub struct ReadFileTool;

#[async_trait]
impl Tool for ReadFileTool {
    fn name(&self) -> &str { "read_file" }
    fn description(&self) -> &str {
        "Read a UTF-8 text file with line numbers. \
         Use `offset` and `limit` to page through large files."
    }
    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["path"],
            "properties": {
                "path":   {"type": "string", "description": "Relative or absolute path"},
                "offset": {"type": "integer", "minimum": 0, "description": "First line (0-based)"},
                "limit":  {"type": "integer", "minimum": 1, "default": 2000,
                           "description": "Max lines to return"}
            }
        })
    }
    async fn execute(&self, input: Value, ctx: &ToolContext) -> ToolOutput {
        #[derive(serde::Deserialize)]
        struct A { path: String, offset: Option<usize>, limit: Option<usize> }
        let a: A = match serde_json::from_value(input) {
            Ok(a) => a,
            Err(e) => return ToolOutput::err(e.to_string()),
        };
        let path = resolve(&ctx.cwd, &a.path);
        let body = match tokio::fs::read_to_string(&path).await {
            Ok(b) => b,
            Err(e) => return ToolOutput::err(format!("read {}: {e}", path.display())),
        };
        let lines: Vec<&str> = body.lines().collect();
        let total = lines.len();
        let start = a.offset.unwrap_or(0).min(total);
        let end = (start + a.limit.unwrap_or(2000)).min(total);
        let mut out = String::with_capacity(end - start + 64);
        for (i, l) in lines[start..end].iter().enumerate() {
            out.push_str(&format!("{:>6}\t{}\n", start + i + 1, l));
        }
        if end < total {
            out.push_str(&format!("\n… [{} more lines, use offset={}]\n", total - end, end));
        }
        ToolOutput::ok(out)
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// list_dir
// ──────────────────────────────────────────────────────────────────────────────

pub struct ListDirTool;

#[async_trait]
impl Tool for ListDirTool {
    fn name(&self) -> &str { "list_dir" }
    fn description(&self) -> &str {
        "List directory contents. Respects .gitignore. \
         Returns a tree-like listing up to max_depth levels."
    }
    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["path"],
            "properties": {
                "path":      {"type": "string"},
                "max_depth": {"type": "integer", "default": 3, "minimum": 1, "maximum": 10}
            }
        })
    }
    async fn execute(&self, input: Value, ctx: &ToolContext) -> ToolOutput {
        #[derive(serde::Deserialize)]
        struct A { path: String, #[serde(default = "d3")] max_depth: usize }
        fn d3() -> usize { 3 }
        let a: A = match serde_json::from_value(input) {
            Ok(a) => a,
            Err(e) => return ToolOutput::err(e.to_string()),
        };
        let root = resolve(&ctx.cwd, &a.path);
        let walker = ignore::WalkBuilder::new(&root)
            .max_depth(Some(a.max_depth))
            .git_ignore(true)
            .build();
        let mut out = String::new();
        let mut count = 0usize;
        for entry in walker.flatten() {
            let rel = entry.path().strip_prefix(&root).unwrap_or(entry.path());
            if rel.as_os_str().is_empty() { continue; }
            let depth = rel.components().count().saturating_sub(1);
            let indent = "  ".repeat(depth);
            let name = rel.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("?");
            let suffix = if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                "/"
            } else {
                ""
            };
            out.push_str(&format!("{indent}{name}{suffix}\n"));
            count += 1;
            if count >= 2000 {
                out.push_str("… [truncated at 2000 entries]\n");
                break;
            }
        }
        ToolOutput::ok(out)
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// write_file
// ──────────────────────────────────────────────────────────────────────────────

pub struct WriteFileTool;

#[async_trait]
impl Tool for WriteFileTool {
    fn name(&self) -> &str { "write_file" }
    fn description(&self) -> &str {
        "Create a new file. Fails if the file already exists. \
         Use edit_file to modify an existing file."
    }
    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["path", "content"],
            "properties": {
                "path":    {"type": "string"},
                "content": {"type": "string"}
            }
        })
    }
    async fn execute(&self, input: Value, ctx: &ToolContext) -> ToolOutput {
        #[derive(serde::Deserialize)]
        struct A { path: String, content: String }
        let a: A = match serde_json::from_value(input) {
            Ok(a) => a,
            Err(e) => return ToolOutput::err(e.to_string()),
        };
        let path = resolve(&ctx.cwd, &a.path);
        if path.exists() {
            return ToolOutput::err(format!(
                "refusing to overwrite {}; use edit_file to modify existing files",
                path.display()
            ));
        }
        if let Some(p) = path.parent() {
            if let Err(e) = tokio::fs::create_dir_all(p).await {
                return ToolOutput::err(format!("mkdir {}: {e}", p.display()));
            }
        }
        if let Err(e) = tokio::fs::write(&path, a.content.as_bytes()).await {
            return ToolOutput::err(format!("write {}: {e}", path.display()));
        }
        ToolOutput::ok(format!(
            "written: {} ({} bytes)",
            path.display(),
            a.content.len()
        ))
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// edit_file
// ──────────────────────────────────────────────────────────────────────────────

pub struct EditFileTool;

#[async_trait]
impl Tool for EditFileTool {
    fn name(&self) -> &str { "edit_file" }
    fn description(&self) -> &str {
        "Replace exact text in an existing file. \
         `old_string` must be unique (or set replace_all=true). \
         Uses atomic rename to prevent partial writes."
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
        let a: A = match serde_json::from_value(input) {
            Ok(a) => a,
            Err(e) => return ToolOutput::err(e.to_string()),
        };
        if a.old_string == a.new_string {
            return ToolOutput::err("old_string and new_string are identical — nothing to do");
        }
        let path = resolve(&ctx.cwd, &a.path);
        let body = match tokio::fs::read_to_string(&path).await {
            Ok(b) => b,
            Err(e) => return ToolOutput::err(format!("read {}: {e}", path.display())),
        };
        let count = body.matches(a.old_string.as_str()).count();
        if count == 0 {
            return ToolOutput::err(
                "old_string not found in file — check for whitespace or line-ending differences",
            );
        }
        if count > 1 && !a.replace_all {
            return ToolOutput::err(format!(
                "old_string matches {count} times — add surrounding context to make it unique, \
                 or set replace_all=true to replace all occurrences"
            ));
        }
        let new_body = if a.replace_all {
            body.replace(a.old_string.as_str(), &a.new_string)
        } else {
            body.replacen(a.old_string.as_str(), &a.new_string, 1)
        };

        // Atomic write via tmp file + rename
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
        ToolOutput::ok(format!(
            "edited: {} ({} replacement{})",
            path.display(),
            count,
            if count == 1 { "" } else { "s" }
        ))
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// run_bash — enterprise-grade shell execution
// ──────────────────────────────────────────────────────────────────────────────

pub struct RunBashTool;

#[async_trait]
impl Tool for RunBashTool {
    fn name(&self) -> &str { "run_bash" }
    fn description(&self) -> &str {
        "Execute a shell command. stdout and stderr are captured and returned. \
         Commands run in the session's working directory. \
         Prefer specific tools (read_file, edit_file, grep) over shell for file operations."
    }
    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["command"],
            "properties": {
                "command": {
                    "type": "string",
                    "description": "Shell command to execute (passed to bash -c)"
                },
                "timeout_secs": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 300,
                    "default": 30,
                    "description": "Seconds before the command is killed"
                }
            }
        })
    }
    async fn execute(&self, input: Value, ctx: &ToolContext) -> ToolOutput {
        #[derive(serde::Deserialize)]
        struct A {
            command: String,
            #[serde(default = "default_timeout")]
            timeout_secs: u64,
        }
        fn default_timeout() -> u64 { 30 }

        let a: A = match serde_json::from_value(input) {
            Ok(a) => a,
            Err(e) => return ToolOutput::err(e.to_string()),
        };

        tracing::debug!(cmd = %a.command, cwd = %ctx.cwd.display(), "run_bash");

        let timeout = std::time::Duration::from_secs(a.timeout_secs);
        let spawn = tokio::process::Command::new("bash")
            .arg("-c")
            .arg(&a.command)
            .current_dir(&ctx.cwd)
            .output();

        match tokio::time::timeout(timeout, spawn).await {
            Err(_elapsed) => ToolOutput::err(format!(
                "command timed out after {timeout_secs}s: {cmd}",
                timeout_secs = a.timeout_secs,
                cmd = a.command,
            )),
            Ok(Err(spawn_err)) => {
                ToolOutput::err(format!("spawn bash: {spawn_err}"))
            }
            Ok(Ok(out)) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let stderr = String::from_utf8_lossy(&out.stderr);

                // Cap each stream at 32 KiB to avoid context explosion
                const CAP: usize = 32 * 1024;
                let stdout = cap_str(&stdout, CAP);
                let stderr = cap_str(&stderr, CAP);

                let mut body = String::new();
                if !stdout.is_empty() {
                    body.push_str(&stdout);
                }
                if !stderr.is_empty() {
                    if !body.is_empty() { body.push('\n'); }
                    body.push_str("--- stderr ---\n");
                    body.push_str(&stderr);
                }
                if body.is_empty() {
                    body.push_str("(no output)");
                }

                if out.status.success() {
                    ToolOutput::ok(body)
                } else {
                    let code = out.status.code().unwrap_or(-1);
                    ToolOutput::err(format!("exit {code}\n{body}"))
                }
            }
        }
    }
}

fn cap_str(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        s.to_string()
    } else {
        // truncate at char boundary
        let end = s.char_indices()
            .map(|(i, _)| i)
            .take_while(|&i| i <= max_bytes)
            .last()
            .unwrap_or(max_bytes);
        format!("{}\n… [output truncated at {} bytes]", &s[..end], max_bytes)
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// grep — regex search across a directory tree
// ──────────────────────────────────────────────────────────────────────────────

pub struct GrepTool;

#[async_trait]
impl Tool for GrepTool {
    fn name(&self) -> &str { "grep" }
    fn description(&self) -> &str {
        "Search for a regex pattern across files in a directory. \
         Respects .gitignore. Returns matching lines with file:line context."
    }
    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["pattern"],
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Regular expression to search for"
                },
                "path": {
                    "type": "string",
                    "description": "Directory or file to search (default: current dir)"
                },
                "glob": {
                    "type": "string",
                    "description": "File glob filter, e.g. \"*.rs\" or \"**/*.toml\""
                },
                "case_insensitive": {
                    "type": "boolean",
                    "default": false
                },
                "max_results": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 1000,
                    "default": 200
                }
            }
        })
    }
    async fn execute(&self, input: Value, ctx: &ToolContext) -> ToolOutput {
        #[derive(serde::Deserialize)]
        struct A {
            pattern: String,
            #[serde(default)]
            path: Option<String>,
            #[serde(default)]
            glob: Option<String>,
            #[serde(default)]
            case_insensitive: bool,
            #[serde(default = "default_max")]
            max_results: usize,
        }
        fn default_max() -> usize { 200 }

        let a: A = match serde_json::from_value(input) {
            Ok(a) => a,
            Err(e) => return ToolOutput::err(e.to_string()),
        };

        let re = match Regex::new(&if a.case_insensitive {
            format!("(?i){}", a.pattern)
        } else {
            a.pattern.clone()
        }) {
            Ok(r) => r,
            Err(e) => return ToolOutput::err(format!("invalid regex: {e}")),
        };

        let search_root = resolve(&ctx.cwd, a.path.as_deref().unwrap_or("."));

        // Build glob override if requested
        let mut builder = ignore::WalkBuilder::new(&search_root);
        builder.git_ignore(true).hidden(false);
        if let Some(g) = &a.glob {
            let mut ov = ignore::overrides::OverrideBuilder::new(&search_root);
            if let Err(e) = ov.add(g) {
                return ToolOutput::err(format!("invalid glob: {e}"));
            }
            match ov.build() {
                Ok(o) => { builder.overrides(o); }
                Err(e) => return ToolOutput::err(format!("glob build: {e}")),
            }
        }

        // Collect in a blocking thread to avoid blocking async executor
        let re2 = re.clone();
        let max = a.max_results;
        let pattern_str = a.pattern.clone();
        let walk = builder.build();

        let result = tokio::task::spawn_blocking(move || -> anyhow::Result<String> {
            let mut out = String::new();
            let mut total = 0usize;
            let mut files_searched = 0usize;
            let mut truncated = false;

            for entry in walk.flatten() {
                if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    continue;
                }
                files_searched += 1;
                let path = entry.path().to_path_buf();
                let content = match std::fs::read_to_string(&path) {
                    Ok(c) => c,
                    Err(_) => continue, // binary or unreadable
                };
                for (lineno, line) in content.lines().enumerate() {
                    if re2.is_match(line) {
                        let display = path.display();
                        out.push_str(&format!("{display}:{n}: {line}\n", n = lineno + 1));
                        total += 1;
                        if total >= max {
                            truncated = true;
                            break;
                        }
                    }
                }
                if truncated { break; }
            }

            if total == 0 {
                return Ok(format!(
                    "No matches for `{pattern_str}` in {files_searched} file(s) searched."
                ));
            }
            if truncated {
                out.push_str(&format!(
                    "\n… [results truncated at {max} matches — refine pattern or use glob]"
                ));
            } else {
                out.push_str(&format!("\n{total} match(es) across {files_searched} file(s)."));
            }
            Ok(out)
        })
        .await;

        match result {
            Ok(Ok(s)) => ToolOutput::ok(s),
            Ok(Err(e)) => ToolOutput::err(e.to_string()),
            Err(e) => ToolOutput::err(format!("grep task: {e}")),
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// glob — find files matching a pattern
// ──────────────────────────────────────────────────────────────────────────────

pub struct GlobTool;

#[async_trait]
impl Tool for GlobTool {
    fn name(&self) -> &str { "glob" }
    fn description(&self) -> &str {
        "Find files by glob pattern (e.g. \"src/**/*.rs\"). \
         Returns paths sorted by modification time (newest first). \
         Respects .gitignore."
    }
    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["pattern"],
            "properties": {
                "pattern": {"type": "string", "description": "Glob pattern"},
                "path":    {"type": "string", "description": "Root directory (default: cwd)"}
            }
        })
    }
    async fn execute(&self, input: Value, ctx: &ToolContext) -> ToolOutput {
        #[derive(serde::Deserialize)]
        struct A { pattern: String, #[serde(default)] path: Option<String> }
        let a: A = match serde_json::from_value(input) {
            Ok(a) => a,
            Err(e) => return ToolOutput::err(e.to_string()),
        };
        let root = resolve(&ctx.cwd, a.path.as_deref().unwrap_or("."));

        let mut builder = ignore::WalkBuilder::new(&root);
        builder.git_ignore(true).hidden(false);
        let mut ov = ignore::overrides::OverrideBuilder::new(&root);
        if let Err(e) = ov.add(&a.pattern) {
            return ToolOutput::err(format!("invalid glob: {e}"));
        }
        match ov.build() {
            Ok(o) => { builder.overrides(o); }
            Err(e) => return ToolOutput::err(format!("glob build: {e}")),
        }

        let walk = builder.build();
        let root2 = root.clone();
        let pat = a.pattern.clone();
        let result = tokio::task::spawn_blocking(move || -> Vec<(std::time::SystemTime, PathBuf)> {
            let mut entries = Vec::new();
            for entry in walk.flatten() {
                if entry.file_type().map(|t| t.is_dir()).unwrap_or(true) { continue; }
                let mtime = entry.metadata().ok()
                    .and_then(|m| m.modified().ok())
                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                entries.push((mtime, entry.path().to_path_buf()));
            }
            entries.sort_by(|a, b| b.0.cmp(&a.0));
            entries
        })
        .await;

        match result {
            Err(e) => ToolOutput::err(format!("glob task: {e}")),
            Ok(entries) => {
                if entries.is_empty() {
                    return ToolOutput::ok(format!("No files match `{pat}`."));
                }
                let mut out = String::new();
                for (_, p) in entries.iter().take(500) {
                    let rel = p.strip_prefix(&root2).unwrap_or(p);
                    out.push_str(&format!("{}\n", rel.display()));
                }
                if entries.len() > 500 {
                    out.push_str(&format!("… [{} more files]\n", entries.len() - 500));
                }
                ToolOutput::ok(out)
            }
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// default_registry
// ──────────────────────────────────────────────────────────────────────────────

pub fn default_registry() -> ToolRegistry {
    let mut r = ToolRegistry::default();
    r.register(Arc::new(ReadFileTool));
    r.register(Arc::new(ListDirTool));
    r.register(Arc::new(WriteFileTool));
    r.register(Arc::new(EditFileTool));
    r.register(Arc::new(RunBashTool));
    r.register(Arc::new(GrepTool));
    r.register(Arc::new(GlobTool));
    r
}

// ──────────────────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use mcc_core::ToolContext;
    use serde_json::json;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn ctx(dir: &TempDir) -> ToolContext {
        ToolContext {
            cwd: dir.path().to_path_buf(),
            session_id: "test".into(),
            depth: 0,
        }
    }

    #[tokio::test]
    async fn test_write_then_read() {
        let dir = TempDir::new().unwrap();
        let ctx = ctx(&dir);

        let out = WriteFileTool
            .execute(json!({"path": "hello.txt", "content": "line1\nline2\n"}), &ctx)
            .await;
        assert!(!out.is_error, "{}", out.content);

        let out = ReadFileTool
            .execute(json!({"path": "hello.txt"}), &ctx)
            .await;
        assert!(!out.is_error);
        assert!(out.content.contains("line1"));
        assert!(out.content.contains("     1\t"));
    }

    #[tokio::test]
    async fn test_write_refuses_overwrite() {
        let dir = TempDir::new().unwrap();
        let ctx = ctx(&dir);
        std::fs::write(dir.path().join("exists.txt"), b"content").unwrap();

        let out = WriteFileTool
            .execute(json!({"path": "exists.txt", "content": "new"}), &ctx)
            .await;
        assert!(out.is_error);
        assert!(out.content.contains("refusing"));
    }

    #[tokio::test]
    async fn test_edit_unique() {
        let dir = TempDir::new().unwrap();
        let ctx = ctx(&dir);
        std::fs::write(dir.path().join("f.rs"), b"fn main() {}").unwrap();

        let out = EditFileTool
            .execute(
                json!({"path": "f.rs", "old_string": "fn main()", "new_string": "fn run()"}),
                &ctx,
            )
            .await;
        assert!(!out.is_error, "{}", out.content);
        let body = std::fs::read_to_string(dir.path().join("f.rs")).unwrap();
        assert!(body.contains("fn run()"));
    }

    #[tokio::test]
    async fn test_edit_not_unique_fails() {
        let dir = TempDir::new().unwrap();
        let ctx = ctx(&dir);
        std::fs::write(dir.path().join("dup.txt"), b"a\na\n").unwrap();

        let out = EditFileTool
            .execute(
                json!({"path": "dup.txt", "old_string": "a", "new_string": "b"}),
                &ctx,
            )
            .await;
        assert!(out.is_error);
        assert!(out.content.contains("2 times"));
    }

    #[tokio::test]
    async fn test_run_bash_echo() {
        let dir = TempDir::new().unwrap();
        let ctx = ctx(&dir);
        let out = RunBashTool
            .execute(json!({"command": "echo hello-enterprise"}), &ctx)
            .await;
        assert!(!out.is_error, "{}", out.content);
        assert!(out.content.contains("hello-enterprise"));
    }

    #[tokio::test]
    async fn test_run_bash_timeout() {
        let dir = TempDir::new().unwrap();
        let ctx = ctx(&dir);
        let out = RunBashTool
            .execute(json!({"command": "sleep 60", "timeout_secs": 1}), &ctx)
            .await;
        assert!(out.is_error);
        assert!(out.content.contains("timed out"));
    }

    #[tokio::test]
    async fn test_run_bash_nonzero_exit() {
        let dir = TempDir::new().unwrap();
        let ctx = ctx(&dir);
        let out = RunBashTool
            .execute(json!({"command": "exit 42"}), &ctx)
            .await;
        assert!(out.is_error);
        assert!(out.content.contains("exit 42") || out.content.contains("42"));
    }

    #[tokio::test]
    async fn test_grep_basic() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("src.rs"), b"fn main() {}\nfn helper() {}").unwrap();
        let ctx = ctx(&dir);

        let out = GrepTool
            .execute(json!({"pattern": "fn \\w+\\(\\)"}), &ctx)
            .await;
        assert!(!out.is_error, "{}", out.content);
        assert!(out.content.contains("src.rs:1:"));
        assert!(out.content.contains("fn main()"));
    }

    #[tokio::test]
    async fn test_grep_no_match() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("f.txt"), b"hello world").unwrap();
        let ctx = ctx(&dir);

        let out = GrepTool
            .execute(json!({"pattern": "xyz_not_found"}), &ctx)
            .await;
        assert!(!out.is_error);
        assert!(out.content.contains("No matches"));
    }
}
