# 第 12 章 Hooks：事件驱动的可扩展点

> Hooks 让团队**不改核心代码**就能定制 Agent 行为——这是一个成熟 harness 的必备能力。

## 12.1 Hooks 是什么

Claude Code 的 hooks 是写在 `settings.json` 里的 shell 命令，在 Agent 生命周期的关键事件触发：

```json
{
  "hooks": {
    "PreToolUse":  [{ "matcher": "Bash(git *)", "command": "bash .claude/hooks/audit-git.sh" }],
    "PostToolUse": [{ "matcher": "Write(.*)",   "command": "bash .claude/hooks/format-file.sh" }],
    "UserPromptSubmit": [{ "command": "bash .claude/hooks/inject-ticket.sh" }],
    "Stop": [{ "command": "notify-send 'claude done'" }]
  }
}
```

Hook 可以：
- **观察**：记日志、发监控
- **修改**：往 prompt 里注入额外上下文
- **阻止**：hook 返回非零或特殊 JSON 拒绝操作

## 12.2 事件清单

我们 Rust 版本定义 8 种事件（覆盖 Claude Code 全部 + 一些扩展）：

| 事件 | 时机 | 能否阻止 |
|---|---|---|
| `SessionStart` | 每次新会话 | 否 |
| `UserPromptSubmit` | 用户提交消息后、发给 LLM 前 | 是（附加内容或取消） |
| `PreToolUse` | 工具调用前 | 是（拒绝） |
| `PostToolUse` | 工具调用后 | 否（但可改结果） |
| `PreCompact` | 上下文压缩前 | 是 |
| `Notification` | 需用户注意（权限询问） | 否 |
| `Stop` | Agent 一轮结束 | 否 |
| `SubagentStop` | 子 Agent 结束 | 否 |

## 12.3 Rust 实现

`examples/12-hooks/src/hooks.rs`：

```rust
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "PascalCase")]
pub enum HookEvent {
    SessionStart { session_id: String, cwd: String },
    UserPromptSubmit { session_id: String, prompt: String },
    PreToolUse { session_id: String, tool_name: String, input: serde_json::Value },
    PostToolUse { session_id: String, tool_name: String, output: String, is_error: bool },
    PreCompact { session_id: String, estimated_tokens: usize },
    Notification { session_id: String, message: String },
    Stop { session_id: String, iterations: u32 },
    SubagentStop { session_id: String, subagent_id: String },
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct HookResponse {
    /// 拒绝操作（仅 Pre* 事件有效）
    #[serde(default)]
    pub block: bool,
    /// 拒绝理由
    #[serde(default)]
    pub reason: Option<String>,
    /// 注入到上下文的额外文本（UserPromptSubmit / PreToolUse）
    #[serde(default)]
    pub inject: Option<String>,
    /// 修改工具输出（PostToolUse）
    #[serde(default)]
    pub replace_output: Option<String>,
}

#[async_trait]
pub trait Hook: Send + Sync {
    fn id(&self) -> &str;
    fn matches(&self, event: &HookEvent) -> bool;
    async fn run(&self, event: &HookEvent) -> anyhow::Result<HookResponse>;
}
```

### 12.3.1 Shell Hook 实现（与 Claude Code 兼容）

```rust
use std::collections::HashMap;

pub struct ShellHook {
    id: String,
    event_type: String,        // "PreToolUse" 等
    matcher: Option<regex::Regex>,
    command: String,
    timeout: std::time::Duration,
}

#[async_trait]
impl Hook for ShellHook {
    fn id(&self) -> &str { &self.id }

    fn matches(&self, event: &HookEvent) -> bool {
        let (ev_name, target) = match event {
            HookEvent::PreToolUse { tool_name, input, .. } => ("PreToolUse", format!("{tool_name}({input})")),
            HookEvent::PostToolUse { tool_name, .. }        => ("PostToolUse", tool_name.clone()),
            HookEvent::UserPromptSubmit { prompt, .. }      => ("UserPromptSubmit", prompt.clone()),
            HookEvent::SessionStart { .. }                  => ("SessionStart", String::new()),
            HookEvent::Stop { .. }                          => ("Stop", String::new()),
            _ => ("Other", String::new()),
        };
        if ev_name != self.event_type { return false; }
        match &self.matcher {
            Some(r) => r.is_match(&target),
            None => true,
        }
    }

    async fn run(&self, event: &HookEvent) -> anyhow::Result<HookResponse> {
        let payload = serde_json::to_string(event)?;

        let mut cmd = tokio::process::Command::new("bash");
        cmd.arg("-c").arg(&self.command)
           .stdin(std::process::Stdio::piped())
           .stdout(std::process::Stdio::piped())
           .stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn()?;
        {
            use tokio::io::AsyncWriteExt;
            let mut stdin = child.stdin.take().unwrap();
            stdin.write_all(payload.as_bytes()).await?;
            stdin.shutdown().await?;
        }

        let out = tokio::time::timeout(self.timeout, child.wait_with_output()).await??;
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();

        // 约定：hook stdout 输出 JSON 形式的 HookResponse
        if !out.status.success() {
            return Ok(HookResponse {
                block: true,
                reason: Some(format!("hook {} failed: {}", self.id, String::from_utf8_lossy(&out.stderr))),
                ..Default::default()
            });
        }
        if stdout.trim().is_empty() {
            return Ok(HookResponse::default());
        }
        Ok(serde_json::from_str(&stdout).unwrap_or_default())
    }
}
```

### 12.3.2 内建 Rust Hook（类型安全 + 快）

对性能敏感的 hook 用原生 Rust 实现。例子：自动对 Rust 写入的文件跑 `rustfmt`：

```rust
pub struct AutoFormatHook;

#[async_trait]
impl Hook for AutoFormatHook {
    fn id(&self) -> &str { "auto_format" }
    fn matches(&self, event: &HookEvent) -> bool {
        matches!(event, HookEvent::PostToolUse { tool_name, .. } if tool_name == "write_file")
    }
    async fn run(&self, event: &HookEvent) -> anyhow::Result<HookResponse> {
        if let HookEvent::PostToolUse { output, .. } = event {
            // 约定 write_file 在 output 里包含 "written: {path}"
            if let Some(path) = output.strip_prefix("written: ") {
                if path.ends_with(".rs") {
                    let _ = tokio::process::Command::new("rustfmt").arg(path).output().await;
                }
            }
        }
        Ok(HookResponse::default())
    }
}
```

## 12.4 Hook Dispatcher

```rust
pub struct HookDispatcher { hooks: Vec<Arc<dyn Hook>> }

impl HookDispatcher {
    pub async fn dispatch(&self, event: HookEvent) -> HookResponse {
        let mut merged = HookResponse::default();
        for h in &self.hooks {
            if !h.matches(&event) { continue; }
            match h.run(&event).await {
                Ok(r) => {
                    if r.block { return r; }                    // 阻止短路
                    if r.inject.is_some() { merged.inject = r.inject; }
                    if r.replace_output.is_some() { merged.replace_output = r.replace_output; }
                }
                Err(e) => {
                    tracing::error!(hook = h.id(), error = %e, "hook failed");
                    // 默认：hook 失败不阻断主流程，但记录
                }
            }
        }
        merged
    }
}
```

## 12.5 在 Agent Loop 中调用

```rust
// PreToolUse
let resp = dispatcher.dispatch(HookEvent::PreToolUse {
    session_id: ctx.session_id.clone(),
    tool_name: name.to_string(),
    input: input.clone(),
}).await;

if resp.block {
    // 合成一个错误 tool_result 返回给模型
    return ToolOutput::err(format!("hook blocked: {}", resp.reason.unwrap_or_default()));
}
if let Some(extra) = resp.inject {
    // 附加到 user context
    …
}

// 真正执行
let out = tool.execute(input, &ctx).await;

// PostToolUse
let post = dispatcher.dispatch(HookEvent::PostToolUse {
    session_id: ctx.session_id.clone(),
    tool_name: name.to_string(),
    output: out.content.clone(),
    is_error: out.is_error,
}).await;

let final_content = post.replace_output.unwrap_or(out.content);
```

## 12.6 实用 Hook 模板

### 12.6.1 拦截所有对 `.env` 的读

Bash hook 脚本 `hooks/deny-env.sh`：

```bash
#!/usr/bin/env bash
payload=$(cat)
path=$(echo "$payload" | jq -r '.input.path // ""')
if [[ "$path" == *".env"* ]]; then
  echo '{"block":true,"reason":"secrets file access denied"}'
else
  echo '{}'
fi
```

### 12.6.2 每轮末尾记录成本

```bash
#!/usr/bin/env bash
payload=$(cat)
session=$(echo "$payload" | jq -r '.session_id')
echo "$payload" >> .logs/stop-$session.jsonl
echo '{}'
```

### 12.6.3 注入 Jira 工单内容

当用户消息里含 `ABC-123` 样式的工单号时自动拉取：

```bash
#!/usr/bin/env bash
payload=$(cat)
prompt=$(echo "$payload" | jq -r '.prompt')
ticket=$(echo "$prompt" | grep -oE '[A-Z]+-[0-9]+' | head -1)
if [[ -n "$ticket" ]]; then
  body=$(curl -s "$JIRA_API/issue/$ticket" | jq -r '.fields.description')
  jq -n --arg body "$body" '{"inject": ("<ticket>\($body)</ticket>")}'
else
  echo '{}'
fi
```

## 12.7 安全考虑

Hook 自身是在用户机器上执行代码，必须：

- Hook 脚本文件权限可控（不允许 LLM 写入 hook 目录）
- Hook 执行超时（默认 30s）
- Hook 路径来自**项目配置**，不来自 LLM 输出
- 记录 hook 执行历史供审计

## 12.8 小结

- Hook = 事件 + 匹配器 + 命令
- 8 种核心事件，每种有清晰的语义
- 内建 Rust hook 高性能，shell hook 给用户自定义
- 是企业级 Agent 的"扩展点":团队规范、合规、CI 集成全靠它

> **下一章**：Skills / Slash Commands —— 比 hooks 更高层的"可打包能力"。

