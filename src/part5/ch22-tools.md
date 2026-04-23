# 第 22 章 工具系统：Read / Write / Edit / Bash / Grep

> 一个编码 Agent 好不好用，**工具集设计**占一半。我们实现 Claude Code 最核心的 7 个工具。

## 22.1 工具清单

| 工具 | 副作用 | 用途 |
|---|---|---|
| `read_file` | 无 | 读取单文件（带行号） |
| `list_dir` | 无 | 列目录（遵循 gitignore） |
| `grep` | 无 | ripgrep 式搜索 |
| `write_file` | 有 | 创建新文件 |
| `edit_file` | 有 | 精确替换（SEARCH/REPLACE） |
| `run_bash` | 有 | 执行 shell 命令 |
| `spawn_subagent` | 有 | 分派子任务（第 26 章） |

所有工具都实现 `mcc_core::Tool` trait（第 6 章定义）。

## 22.2 `read_file`（改良版）

关键改进：大文件自动分页 + 行号前缀 + 超长行截断。

```rust
pub struct ReadFileTool;

#[async_trait]
impl Tool for ReadFileTool {
    fn name(&self) -> &str { "read_file" }
    fn description(&self) -> &str {
        "Read a UTF-8 text file with line numbers. Use `offset`/`limit` for large files. \
        Default returns up to 2000 lines. Lines longer than 2000 chars are truncated."
    }
    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type":"object","required":["path"],
            "properties":{
                "path":{"type":"string"},
                "offset":{"type":"integer","minimum":0},
                "limit":{"type":"integer","minimum":1,"default":2000}
            }
        })
    }
    async fn execute(&self, input: serde_json::Value, ctx: &ToolContext) -> ToolOutput {
        #[derive(serde::Deserialize)] struct A { path: String, offset: Option<usize>, limit: Option<usize> }
        let a: A = match serde_json::from_value(input) { Ok(a)=>a, Err(e)=>return ToolOutput::err(e.to_string()) };

        let path = resolve_path(&ctx.cwd, &a.path);
        let body = match tokio::fs::read_to_string(&path).await {
            Ok(b) => b,
            Err(e) => return ToolOutput::err(format!("read {}: {e}", path.display())),
        };

        let lines: Vec<&str> = body.lines().collect();
        let total = lines.len();
        let start = a.offset.unwrap_or(0).min(total);
        let end = (start + a.limit.unwrap_or(2000)).min(total);

        let mut out = String::new();
        for (i, line) in lines[start..end].iter().enumerate() {
            let truncated = if line.len() > 2000 { format!("{}… [truncated]", &line[..2000]) } else { line.to_string() };
            out.push_str(&format!("{:>6}\t{}\n", start + i + 1, truncated));
        }
        if end < total {
            out.push_str(&format!("\n… [{} more lines, use offset={} to continue]\n", total - end, end));
        }
        ToolOutput::ok(out)
    }
}

fn resolve_path(cwd: &std::path::Path, p: &str) -> std::path::PathBuf {
    let pb = std::path::PathBuf::from(p);
    if pb.is_absolute() { pb } else { cwd.join(pb) }
}
```

## 22.3 `list_dir` 与 `grep`

`list_dir` 使用 `ignore::WalkBuilder`（遵循 `.gitignore`），实现见第 6 章。

`grep` 集成 `ignore` + `grep-regex` + `grep-searcher`（ripgrep 的底层库）：

```rust
use grep::regex::RegexMatcher;
use grep::searcher::{Searcher, Sink, SinkMatch};
use ignore::WalkBuilder;

pub struct GrepTool;

#[async_trait]
impl Tool for GrepTool {
    fn name(&self) -> &str { "grep" }
    fn description(&self) -> &str { "Search a regex pattern across files. Respects .gitignore." }
    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type":"object","required":["pattern"],
            "properties":{
                "pattern":{"type":"string","description":"Rust regex"},
                "path":{"type":"string","description":"Root to search (default=cwd)"},
                "glob":{"type":"string","description":"Only files matching glob"},
                "max_results":{"type":"integer","default":100}
            }
        })
    }
    async fn execute(&self, input: serde_json::Value, ctx: &ToolContext) -> ToolOutput {
        #[derive(serde::Deserialize)]
        struct A { pattern: String, path: Option<String>, glob: Option<String>, max_results: Option<usize> }
        let a: A = match serde_json::from_value(input) { Ok(a)=>a, Err(e)=>return ToolOutput::err(e.to_string()) };

        let matcher = match RegexMatcher::new_line_matcher(&a.pattern) {
            Ok(m) => m, Err(e) => return ToolOutput::err(format!("bad regex: {e}")),
        };
        let root = a.path.map(|p| resolve_path(&ctx.cwd, &p)).unwrap_or_else(|| ctx.cwd.clone());
        let mut wb = WalkBuilder::new(&root);
        wb.hidden(false).git_ignore(true);
        if let Some(g) = a.glob {
            let mut ob = ignore::overrides::OverrideBuilder::new(&root);
            let _ = ob.add(&g);
            wb.overrides(ob.build().unwrap());
        }

        let max = a.max_results.unwrap_or(100);
        let results = std::sync::Mutex::new(Vec::<String>::new());

        wb.build_parallel().run(|| {
            let matcher = matcher.clone();
            let results = &results;
            Box::new(move |entry| {
                let entry = match entry { Ok(e)=>e, Err(_)=>return ignore::WalkState::Continue };
                if !entry.file_type().map_or(false, |t| t.is_file()) { return ignore::WalkState::Continue; }
                let path = entry.path().to_path_buf();
                let mut sink = CollectSink { path: path.clone(), out: results };
                let _ = Searcher::new().search_path(&matcher, &path, &mut sink);
                if results.lock().unwrap().len() >= max { ignore::WalkState::Quit } else { ignore::WalkState::Continue }
            })
        });

        let mut list = results.into_inner().unwrap();
        list.truncate(max);
        if list.is_empty() { ToolOutput::ok("(no matches)".into()) }
        else { ToolOutput::ok(list.join("\n")) }
    }
}

struct CollectSink<'a> { path: std::path::PathBuf, out: &'a std::sync::Mutex<Vec<String>> }
impl<'a> Sink for CollectSink<'a> {
    type Error = std::io::Error;
    fn matched(&mut self, _: &Searcher, m: &SinkMatch<'_>) -> Result<bool, Self::Error> {
        let line = String::from_utf8_lossy(m.bytes()).trim_end().to_string();
        let n = m.line_number().unwrap_or(0);
        self.out.lock().unwrap().push(format!("{}:{}:{}", self.path.display(), n, line));
        Ok(true)
    }
}
```

## 22.4 `write_file`

谨慎设计：**不允许覆盖现有文件**，覆盖要走 `edit_file`。这样 LLM 不会"无声抹掉"用户的代码。

```rust
pub struct WriteFileTool;

#[async_trait]
impl Tool for WriteFileTool {
    fn name(&self) -> &str { "write_file" }
    fn description(&self) -> &str {
        "Create a new file with the given content. \
        FAILS if the file already exists — use `edit_file` for modifications."
    }
    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type":"object","required":["path","content"],
            "properties":{"path":{"type":"string"},"content":{"type":"string"}}
        })
    }
    async fn execute(&self, input: serde_json::Value, ctx: &ToolContext) -> ToolOutput {
        #[derive(serde::Deserialize)] struct A { path: String, content: String }
        let a: A = match serde_json::from_value(input) { Ok(a)=>a, Err(e)=>return ToolOutput::err(e.to_string()) };

        let path = resolve_path(&ctx.cwd, &a.path);
        if path.exists() {
            return ToolOutput::err(format!("refusing to overwrite existing file {}; use edit_file", path.display()));
        }
        if let Some(parent) = path.parent() {
            if let Err(e) = tokio::fs::create_dir_all(parent).await {
                return ToolOutput::err(format!("mkdir: {e}"));
            }
        }
        if let Err(e) = tokio::fs::write(&path, a.content.as_bytes()).await {
            return ToolOutput::err(format!("write: {e}"));
        }
        ToolOutput::ok(format!("written: {} ({} bytes)", path.display(), a.content.len()))
    }
}
```

## 22.5 `edit_file`（精确替换，SEARCH/REPLACE 风格）

这是整个编码 Agent 最关键的工具。用"搜索文本必须唯一命中"来防止误改：

```rust
pub struct EditFileTool;

#[async_trait]
impl Tool for EditFileTool {
    fn name(&self) -> &str { "edit_file" }
    fn description(&self) -> &str {
        "Replace exact text in an existing file. The `old_string` must appear EXACTLY ONCE \
        (include enough surrounding context to be unique). Set `replace_all=true` to replace every occurrence."
    }
    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type":"object","required":["path","old_string","new_string"],
            "properties":{
                "path":{"type":"string"},
                "old_string":{"type":"string"},
                "new_string":{"type":"string"},
                "replace_all":{"type":"boolean","default":false}
            }
        })
    }
    async fn execute(&self, input: serde_json::Value, ctx: &ToolContext) -> ToolOutput {
        #[derive(serde::Deserialize)]
        struct A { path: String, old_string: String, new_string: String, #[serde(default)] replace_all: bool }
        let a: A = match serde_json::from_value(input) { Ok(a)=>a, Err(e)=>return ToolOutput::err(e.to_string()) };

        if a.old_string == a.new_string {
            return ToolOutput::err("old_string equals new_string (no-op)".into());
        }
        let path = resolve_path(&ctx.cwd, &a.path);
        let body = match tokio::fs::read_to_string(&path).await {
            Ok(b) => b, Err(e) => return ToolOutput::err(format!("read: {e}")),
        };

        let occurrences = body.matches(&a.old_string).count();
        if occurrences == 0 {
            return ToolOutput::err("old_string not found (whitespace must match exactly)".into());
        }
        if occurrences > 1 && !a.replace_all {
            return ToolOutput::err(format!(
                "old_string matches {} times; add more context or set replace_all=true", occurrences
            ));
        }

        let new_body = if a.replace_all { body.replace(&a.old_string, &a.new_string) }
                       else { body.replacen(&a.old_string, &a.new_string, 1) };

        // 写临时文件再 rename，避免崩溃中损坏原文件
        let tmp = path.with_extension(format!("{}.mcc-tmp", path.extension().and_then(|s|s.to_str()).unwrap_or("bak")));
        if let Err(e) = tokio::fs::write(&tmp, new_body.as_bytes()).await {
            return ToolOutput::err(format!("write tmp: {e}"));
        }
        if let Err(e) = tokio::fs::rename(&tmp, &path).await {
            let _ = tokio::fs::remove_file(&tmp).await;
            return ToolOutput::err(format!("rename: {e}"));
        }

        ToolOutput::ok(format!("edited: {} ({} replacement{})", path.display(), occurrences, if occurrences==1{""}else{"s"}))
    }
}
```

**设计亮点**：

- **唯一性校验**：多匹配且未 `replace_all` 直接失败
- **原子替换**：写 tmp → rename，崩溃不损坏
- **语义提示**：错误消息明确告诉模型怎么改（加上下文 / 设 `replace_all`）

## 22.6 `run_bash` (生产版)

在第 6 章基础上加：权限检查、超时、截断、streaming 输出事件。

```rust
pub struct RunBashTool {
    pub perms: Arc<PermissionChecker>,
    pub prompter: Arc<dyn UserPrompter>,
    pub event_tx: tokio::sync::mpsc::UnboundedSender<AgentEvent>,
}

#[async_trait]
impl Tool for RunBashTool {
    fn name(&self) -> &str { "run_bash" }
    fn description(&self) -> &str {
        "Execute a bash command. Returns exit_code, stdout, stderr. \
        Use for tests, git, build tools. AVOID: destructive ops without user confirm."
    }
    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type":"object","required":["command"],
            "properties":{
                "command":{"type":"string"},
                "timeout_sec":{"type":"integer","default":120,"minimum":1,"maximum":1800},
                "description":{"type":"string","description":"A one-line summary shown to the user"}
            }
        })
    }
    async fn execute(&self, input: serde_json::Value, ctx: &ToolContext) -> ToolOutput {
        #[derive(serde::Deserialize)]
        struct A { command: String, #[serde(default="t")] timeout_sec: u64, description: Option<String> }
        fn t() -> u64 { 120 }

        let a: A = match serde_json::from_value(input) { Ok(a)=>a, Err(e)=>return ToolOutput::err(e.to_string()) };

        // 权限
        let req = PermissionRequest { category:"Bash".into(), action: Action::Bash { cmd: a.command.clone() } };
        match self.perms.check(&req) {
            Decision::Deny(why) => return ToolOutput::err(format!("denied: {why}")),
            Decision::Ask(_) => {
                let label = a.description.clone().unwrap_or_else(|| a.command.clone());
                if !self.prompter.ask(&format!("run bash: {label}\n> {}", a.command)).await {
                    return ToolOutput::err("user rejected".into());
                }
            }
            Decision::Allow => {}
        }

        use tokio::io::AsyncBufReadExt;
        use tokio::process::Command;

        let mut child = match Command::new("bash")
            .arg("-c").arg(&a.command)
            .current_dir(&ctx.cwd)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn() {
            Ok(c) => c,
            Err(e) => return ToolOutput::err(format!("spawn: {e}")),
        };

        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let mut out_reader = tokio::io::BufReader::new(stdout).lines();
        let mut err_reader = tokio::io::BufReader::new(stderr).lines();
        let mut out_buf = String::new();
        let mut err_buf = String::new();

        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(a.timeout_sec);

        loop {
            tokio::select! {
                line = out_reader.next_line() => {
                    match line {
                        Ok(Some(l)) => { let _ = self.event_tx.send(AgentEvent::Notice(format!("stdout: {l}"))); out_buf.push_str(&l); out_buf.push('\n'); }
                        _ => break,
                    }
                }
                line = err_reader.next_line() => {
                    if let Ok(Some(l)) = line { err_buf.push_str(&l); err_buf.push('\n'); }
                }
                _ = tokio::time::sleep_until(deadline) => {
                    let _ = child.start_kill();
                    return ToolOutput::err(format!("timeout after {}s", a.timeout_sec));
                }
            }
            if out_buf.len() + err_buf.len() > 256 * 1024 { break; }
        }

        let status = match child.wait().await { Ok(s)=>s, Err(e)=>return ToolOutput::err(format!("wait: {e}")) };
        let body = format!(
            "exit_code: {}\n--- stdout ({} bytes) ---\n{}\n--- stderr ({} bytes) ---\n{}",
            status.code().unwrap_or(-1),
            out_buf.len(), truncate_middle(&out_buf, 32_000),
            err_buf.len(), truncate_middle(&err_buf, 16_000),
        );
        if status.success() { ToolOutput::ok(body) } else { ToolOutput::err(body) }
    }
}
```

## 22.7 工具注册

`mcc-harness::build_registry`：

```rust
pub fn build_registry(
    perms: Arc<PermissionChecker>,
    prompter: Arc<dyn UserPrompter>,
    event_tx: tokio::sync::mpsc::UnboundedSender<AgentEvent>,
) -> ToolRegistry {
    let mut r = ToolRegistry::default();
    r.register(Arc::new(ReadFileTool));
    r.register(Arc::new(ListDirTool));
    r.register(Arc::new(GrepTool));
    r.register(Arc::new(WriteFileTool));
    r.register(Arc::new(EditFileTool));
    r.register(Arc::new(RunBashTool { perms: perms.clone(), prompter: prompter.clone(), event_tx: event_tx.clone() }));
    // SpawnSubagentTool 在第 26 章加
    r
}
```

## 22.8 小结

- 7 个核心工具，读/查/写/改/跑分工明确
- `edit_file` 的"唯一匹配 + 原子替换"是工业级要点
- `run_bash` 集成了权限、超时、streaming、截断
- 所有工具的 `description` 都是给模型看的——措辞是工程

> **下一章**：把工具接入主循环，跑通端到端。

