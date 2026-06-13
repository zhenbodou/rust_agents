# 第 22 章 工具系统：Read / Write / Edit / Bash / Grep / Glob

> 一个编码 Agent 好不好用，**工具集设计**占一半。我们实现 mini-claude-code 的 7 个核心工具。

## 22.1 工具清单

| 工具 | 副作用 | 用途 |
|---|---|---|
| `read_file` | 无 | 读取单文件（带行号 + 分页） |
| `list_dir` | 无 | 列目录（遵循 `.gitignore`） |
| `grep` | 无 | 正则搜索（`regex` crate + `ignore`） |
| `glob` | 无 | 按文件名模式查找，按修改时间排序 |
| `write_file` | 有 | 创建新文件（拒绝覆盖） |
| `edit_file` | 有 | 精确替换（原子 rename） |
| `run_bash` | 有 | 执行 shell 命令（超时 + 输出截断） |

所有工具实现 `mcc_core::Tool` trait，注册到 `ToolRegistry`。

## 22.2 依赖

```toml
# crates/mcc-tools/Cargo.toml
[dependencies]
mcc-core    = { path = "../mcc-core" }
tokio       = { workspace = true, features = ["process"] }
serde.workspace = true
serde_json.workspace = true
anyhow.workspace = true
async-trait.workspace = true
ignore.workspace = true   # .gitignore 遵守 + 目录遍历
regex.workspace = true    # grep 正则引擎

[dev-dependencies]
tempfile.workspace = true
```

> **为什么不用 `grep-regex`（ripgrep 底层库）？**  
> ripgrep 的底层库（`grep-regex` + `grep-searcher`）提供并行搜索，是生产级选择。  
> 但它依赖链较重且 API 复杂。`regex` + `ignore` 已满足大多数场景，且 API 简洁、测试易写。  
> 如需处理巨型 monorepo（10M+ 行），可将 `GrepTool` 的 `spawn_blocking` 替换为 `WalkParallel`。

## 22.3 `read_file`

关键改进：大文件分页 + 行号前缀 + 截断提示。

```rust
pub struct ReadFileTool;

#[async_trait]
impl Tool for ReadFileTool {
    fn name(&self) -> &str { "read_file" }
    fn description(&self) -> &str {
        "Read a UTF-8 text file with line numbers. \
         Use `offset`/`limit` to page through large files. \
         Default: up to 2000 lines."
    }
    fn input_schema(&self) -> Value {
        json!({
            "type": "object", "required": ["path"],
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
        let a: A = match serde_json::from_value(input) {
            Ok(a) => a, Err(e) => return ToolOutput::err(e.to_string()),
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
        let mut out = String::new();
        for (i, l) in lines[start..end].iter().enumerate() {
            out.push_str(&format!("{:>6}\t{}\n", start + i + 1, l));
        }
        if end < total {
            out.push_str(&format!("\n… [{} more lines, use offset={}]\n", total - end, end));
        }
        ToolOutput::ok(out)
    }
}
```

## 22.4 `list_dir`

使用 `ignore::WalkBuilder`（遵循 `.gitignore`），输出树形缩进。

```rust
pub struct ListDirTool;

#[async_trait]
impl Tool for ListDirTool {
    fn name(&self) -> &str { "list_dir" }
    fn description(&self) -> &str {
        "List directory contents. Respects .gitignore. \
         Returns a tree-like listing up to max_depth levels."
    }
    // ... input_schema 省略
    async fn execute(&self, input: Value, ctx: &ToolContext) -> ToolOutput {
        let root = resolve(&ctx.cwd, &a.path);
        let walker = ignore::WalkBuilder::new(&root)
            .max_depth(Some(a.max_depth))
            .git_ignore(true)
            .build();
        let mut out = String::new();
        for entry in walker.flatten() {
            let rel = entry.path().strip_prefix(&root).unwrap_or(entry.path());
            let depth = rel.components().count().saturating_sub(1);
            let indent = "  ".repeat(depth);
            let name = rel.file_name().and_then(|n| n.to_str()).unwrap_or("?");
            let suffix = if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) { "/" } else { "" };
            out.push_str(&format!("{indent}{name}{suffix}\n"));
        }
        ToolOutput::ok(out)
    }
}
```

## 22.5 `grep` — 基于 `regex` + `ignore`

核心思路：在 `spawn_blocking` 里同步遍历文件树 + 匹配，避免阻塞 async executor。

```rust
pub struct GrepTool;

#[async_trait]
impl Tool for GrepTool {
    fn name(&self) -> &str { "grep" }
    fn description(&self) -> &str {
        "Search for a regex pattern across files. \
         Respects .gitignore. Returns matching lines with file:line context."
    }
    fn input_schema(&self) -> Value {
        json!({
            "type": "object", "required": ["pattern"],
            "properties": {
                "pattern":          {"type": "string", "description": "Regular expression"},
                "path":             {"type": "string", "description": "Root dir (default: cwd)"},
                "glob":             {"type": "string", "description": "File glob filter, e.g. \"*.rs\""},
                "case_insensitive": {"type": "boolean", "default": false},
                "max_results":      {"type": "integer", "default": 200}
            }
        })
    }
    async fn execute(&self, input: Value, ctx: &ToolContext) -> ToolOutput {
        // ... parse args

        // 支持大小写不敏感：用 (?i) 前缀
        let re = match Regex::new(&if a.case_insensitive {
            format!("(?i){}", a.pattern)
        } else {
            a.pattern.clone()
        }) {
            Ok(r) => r,
            Err(e) => return ToolOutput::err(format!("invalid regex: {e}")),
        };

        let search_root = resolve(&ctx.cwd, a.path.as_deref().unwrap_or("."));

        // 可选 glob 过滤
        let mut builder = ignore::WalkBuilder::new(&search_root);
        builder.git_ignore(true).hidden(false);
        if let Some(g) = &a.glob {
            let mut ov = ignore::overrides::OverrideBuilder::new(&search_root);
            let _ = ov.add(g.as_str());
            if let Ok(o) = ov.build() {
                builder.overrides(o);
            }
        }

        // 同步遍历在 spawn_blocking 中完成
        let walk = builder.build();
        let result = tokio::task::spawn_blocking(move || -> anyhow::Result<String> {
            let mut out = String::new();
            let mut total = 0usize;
            let mut files_searched = 0usize;

            for entry in walk.flatten() {
                if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) { continue; }
                files_searched += 1;
                let content = match std::fs::read_to_string(entry.path()) {
                    Ok(c) => c, Err(_) => continue,  // skip binary
                };
                for (lineno, line) in content.lines().enumerate() {
                    if re.is_match(line) {
                        out.push_str(&format!(
                            "{}:{}:{}\n",
                            entry.path().display(), lineno + 1, line
                        ));
                        total += 1;
                        if total >= a.max_results {
                            out.push_str("… [truncated]\n");
                            return Ok(out);
                        }
                    }
                }
            }
            if total == 0 {
                return Ok(format!("No matches in {files_searched} file(s)."));
            }
            out.push_str(&format!("\n{total} match(es) in {files_searched} file(s)."));
            Ok(out)
        }).await;

        match result {
            Ok(Ok(s)) => ToolOutput::ok(s),
            Ok(Err(e)) => ToolOutput::err(e.to_string()),
            Err(e) => ToolOutput::err(format!("grep task panicked: {e}")),
        }
    }
}
```

**设计要点：**

- `ignore::overrides::OverrideBuilder` — glob 白名单，`.gitignore` 自动跳过
- `spawn_blocking` — 文件 I/O 是同步的，不能直接在 async task 里循环
- `regex::Regex::is_match` — 零拷贝，只需 `&str`

## 22.6 `glob` — 文件名模式查找

`glob` 补充了 `grep` 的盲区：**"找到文件"** vs **"在文件里找内容"**。

```rust
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
            "type": "object", "required": ["pattern"],
            "properties": {
                "pattern": {"type": "string", "description": "Glob pattern like **/*.toml"},
                "path":    {"type": "string", "description": "Root directory (default: cwd)"}
            }
        })
    }
    async fn execute(&self, input: Value, ctx: &ToolContext) -> ToolOutput {
        let root = resolve(&ctx.cwd, a.path.as_deref().unwrap_or("."));

        // 用 overrides 实现 glob 白名单
        let mut builder = ignore::WalkBuilder::new(&root);
        builder.git_ignore(true).hidden(false);
        let mut ov = ignore::overrides::OverrideBuilder::new(&root);
        if let Err(e) = ov.add(a.pattern.as_str()) {
            return ToolOutput::err(format!("invalid glob: {e}"));
        }
        match ov.build() {
            Ok(o) => { builder.overrides(o); }
            Err(e) => return ToolOutput::err(format!("glob build: {e}")),
        }

        let walk = builder.build();
        // 收集并按 mtime 排序
        let result = tokio::task::spawn_blocking(move || {
            let mut entries: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
            for e in walk.flatten() {
                if e.file_type().map(|t| t.is_dir()).unwrap_or(true) { continue; }
                let mtime = e.metadata().ok()
                    .and_then(|m| m.modified().ok())
                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                entries.push((mtime, e.path().to_path_buf()));
            }
            entries.sort_by(|a, b| b.0.cmp(&a.0));
            entries
        }).await;

        match result {
            Err(e) => ToolOutput::err(format!("glob task: {e}")),
            Ok(entries) => {
                if entries.is_empty() {
                    return ToolOutput::ok(format!("No files match `{}`.", a.pattern));
                }
                let mut out = String::new();
                for (_, p) in entries.iter().take(500) {
                    let rel = p.strip_prefix(&root).unwrap_or(p);
                    out.push_str(&format!("{}\n", rel.display()));
                }
                if entries.len() > 500 {
                    out.push_str(&format!("… [{} more]\n", entries.len() - 500));
                }
                ToolOutput::ok(out)
            }
        }
    }
}
```

## 22.7 `write_file`

谨慎设计：**不允许覆盖现有文件**，覆盖走 `edit_file`。这样 LLM 不会"无声抹掉"代码。

```rust
pub struct WriteFileTool;

#[async_trait]
impl Tool for WriteFileTool {
    fn name(&self) -> &str { "write_file" }
    fn description(&self) -> &str {
        "Create a new file. FAILS if the file already exists. \
         Use edit_file to modify existing files."
    }
    async fn execute(&self, input: Value, ctx: &ToolContext) -> ToolOutput {
        let path = resolve(&ctx.cwd, &a.path);
        if path.exists() {
            return ToolOutput::err(format!(
                "refusing to overwrite {}; use edit_file",
                path.display()
            ));
        }
        if let Some(p) = path.parent() {
            tokio::fs::create_dir_all(p).await.ok();
        }
        tokio::fs::write(&path, a.content.as_bytes()).await
            .map(|_| ToolOutput::ok(format!("written: {} ({} bytes)", path.display(), a.content.len())))
            .unwrap_or_else(|e| ToolOutput::err(format!("write: {e}")))
    }
}
```

## 22.8 `edit_file` — 精确替换

最关键的工具，三层防护：

1. **唯一性校验** — `old_string` 必须恰好命中 1 次
2. **空操作检测** — `old == new` 直接失败，避免无意义写入
3. **原子替换** — 写临时文件 → `rename`，崩溃也不损坏原文件

```rust
pub struct EditFileTool;

#[async_trait]
impl Tool for EditFileTool {
    fn name(&self) -> &str { "edit_file" }
    fn description(&self) -> &str {
        "Replace exact text in an existing file. \
         `old_string` must be UNIQUE (add surrounding context if needed). \
         Set `replace_all=true` to replace all occurrences. \
         Uses atomic rename to prevent partial writes."
    }
    async fn execute(&self, input: Value, ctx: &ToolContext) -> ToolOutput {
        if a.old_string == a.new_string {
            return ToolOutput::err("old_string and new_string are identical");
        }
        let path = resolve(&ctx.cwd, &a.path);
        let body = match tokio::fs::read_to_string(&path).await {
            Ok(b) => b, Err(e) => return ToolOutput::err(format!("read: {e}")),
        };
        let count = body.matches(a.old_string.as_str()).count();
        if count == 0 {
            return ToolOutput::err(
                "old_string not found — check whitespace or line endings"
            );
        }
        if count > 1 && !a.replace_all {
            return ToolOutput::err(format!(
                "old_string matches {count} times — add context to make it unique, \
                 or set replace_all=true"
            ));
        }
        let new_body = if a.replace_all {
            body.replace(a.old_string.as_str(), &a.new_string)
        } else {
            body.replacen(a.old_string.as_str(), &a.new_string, 1)
        };
        // Atomic write via temp file + rename
        let tmp = path.with_extension("mcc-tmp");
        if let Err(e) = tokio::fs::write(&tmp, new_body.as_bytes()).await {
            return ToolOutput::err(format!("write tmp: {e}"));
        }
        if let Err(e) = tokio::fs::rename(&tmp, &path).await {
            let _ = tokio::fs::remove_file(&tmp).await;
            return ToolOutput::err(format!("rename: {e}"));
        }
        ToolOutput::ok(format!(
            "edited: {} ({count} replacement{})",
            path.display(),
            if count == 1 { "" } else { "s" }
        ))
    }
}
```

## 22.9 `run_bash` — 带超时的 shell 执行

权限检查由 `AgentLoop::execute_tools_streaming` 统一处理（见第 23 章），工具本身只负责执行。

```rust
pub struct RunBashTool;

#[async_trait]
impl Tool for RunBashTool {
    fn name(&self) -> &str { "run_bash" }
    fn description(&self) -> &str {
        "Execute a shell command. stdout and stderr are captured and returned. \
         Prefer specific tools (read_file, edit_file, grep) over shell for file operations."
    }
    fn input_schema(&self) -> Value {
        json!({
            "type": "object", "required": ["command"],
            "properties": {
                "command":      {"type": "string"},
                "timeout_secs": {
                    "type": "integer", "minimum": 1, "maximum": 300, "default": 30
                }
            }
        })
    }
    async fn execute(&self, input: Value, ctx: &ToolContext) -> ToolOutput {
        let timeout = Duration::from_secs(a.timeout_secs);

        match tokio::time::timeout(
            timeout,
            tokio::process::Command::new("bash")
                .arg("-c").arg(&a.command)
                .current_dir(&ctx.cwd)
                .output(),
        ).await {
            Err(_) => ToolOutput::err(format!(
                "command timed out after {}s", a.timeout_secs
            )),
            Ok(Err(e)) => ToolOutput::err(format!("spawn: {e}")),
            Ok(Ok(out)) => {
                // 32 KiB cap per stream
                let stdout = cap_str(&String::from_utf8_lossy(&out.stdout), 32 * 1024);
                let stderr = cap_str(&String::from_utf8_lossy(&out.stderr), 32 * 1024);

                let mut body = String::new();
                if !stdout.is_empty() { body.push_str(&stdout); }
                if !stderr.is_empty() {
                    if !body.is_empty() { body.push('\n'); }
                    body.push_str("--- stderr ---\n");
                    body.push_str(&stderr);
                }
                if body.is_empty() { body.push_str("(no output)"); }

                if out.status.success() {
                    ToolOutput::ok(body)
                } else {
                    ToolOutput::err(format!("exit {}\n{body}", out.status.code().unwrap_or(-1)))
                }
            }
        }
    }
}

fn cap_str(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes { s.to_string() }
    else {
        let end = s.char_indices().map(|(i,_)| i)
            .take_while(|&i| i <= max_bytes).last().unwrap_or(max_bytes);
        format!("{}\n… [truncated at {max_bytes} bytes]", &s[..end])
    }
}
```

**与第 23 章的分工：**

| 关注点 | 处理层 |
|---|---|
| 执行命令 + 超时 | `RunBashTool` |
| 输出截断 | `RunBashTool` |
| 权限检查（allow/deny/ask） | `AgentLoop::execute_tools_streaming` |
| Streaming 事件（ToolCallEnd） | `AgentLoop::execute_tools_streaming` |

这种分层设计让工具本身**纯粹**，便于单元测试。

## 22.10 工具注册

```rust
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
```

## 22.11 单元测试策略

所有工具都可以在 `tempfile::TempDir` 里测试，无需网络或 LLM：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn ctx(dir: &TempDir) -> ToolContext {
        ToolContext { cwd: dir.path().to_path_buf(), session_id: "test".into(), depth: 0 }
    }

    #[tokio::test]
    async fn test_write_then_read() {
        let dir = TempDir::new().unwrap();
        let out = WriteFileTool.execute(
            json!({"path": "hello.txt", "content": "line1\nline2\n"}), &ctx(&dir)
        ).await;
        assert!(!out.is_error);

        let out = ReadFileTool.execute(json!({"path": "hello.txt"}), &ctx(&dir)).await;
        assert!(out.content.contains("     1\tline1"));
    }

    #[tokio::test]
    async fn test_edit_uniqueness_guard() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("dup.txt"), "a\na\n").unwrap();
        let out = EditFileTool.execute(
            json!({"path": "dup.txt", "old_string": "a", "new_string": "b"}), &ctx(&dir)
        ).await;
        assert!(out.is_error);
        assert!(out.content.contains("2 times"));
    }

    #[tokio::test]
    async fn test_run_bash_timeout() {
        let dir = TempDir::new().unwrap();
        let out = RunBashTool.execute(
            json!({"command": "sleep 60", "timeout_secs": 1}), &ctx(&dir)
        ).await;
        assert!(out.is_error);
        assert!(out.content.contains("timed out"));
    }

    #[tokio::test]
    async fn test_grep_basic() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("src.rs"), "fn main() {}\nfn helper() {}").unwrap();
        let out = GrepTool.execute(
            json!({"pattern": "fn \\w+\\(\\)"}), &ctx(&dir)
        ).await;
        assert!(!out.is_error);
        assert!(out.content.contains("src.rs:1:"));
    }
}
```

运行：
```bash
cargo test -p mcc-tools -- --nocapture
```

## 22.12 小结

- 7 个工具，读/查/写/改/跑分工明确
- `grep` 用 `regex` + `ignore`，`glob` 按 mtime 排序
- `edit_file` 的"唯一匹配 + 原子替换"是工业级关键
- `run_bash` 只管执行，权限层在 `AgentLoop` 统一处理
- 每个工具都有 `TempDir` 单元测试，不依赖 LLM

> **下一章**：把工具接入 `AgentLoop`，跑通端到端流式输出。
