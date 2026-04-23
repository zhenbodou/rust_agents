//! 第 12 章：Hook 系统（事件 + 匹配器 + shell/内建执行）。

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
    Stop { session_id: String, iterations: u32 },
    SubagentStop { session_id: String, subagent_id: String },
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct HookResponse {
    #[serde(default)]
    pub block: bool,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub inject: Option<String>,
    #[serde(default)]
    pub replace_output: Option<String>,
}

#[async_trait]
pub trait Hook: Send + Sync {
    fn id(&self) -> &str;
    fn matches(&self, event: &HookEvent) -> bool;
    async fn run(&self, event: &HookEvent) -> anyhow::Result<HookResponse>;
}

pub struct ShellHook {
    pub id: String,
    pub event_type: String,
    pub matcher: Option<regex::Regex>,
    pub command: String,
    pub timeout: std::time::Duration,
}

#[async_trait]
impl Hook for ShellHook {
    fn id(&self) -> &str { &self.id }

    fn matches(&self, event: &HookEvent) -> bool {
        let (ev_name, target) = match event {
            HookEvent::PreToolUse { tool_name, input, .. } => {
                ("PreToolUse", format!("{tool_name}({input})"))
            }
            HookEvent::PostToolUse { tool_name, .. } => ("PostToolUse", tool_name.clone()),
            HookEvent::UserPromptSubmit { prompt, .. } => ("UserPromptSubmit", prompt.clone()),
            HookEvent::SessionStart { .. } => ("SessionStart", String::new()),
            HookEvent::Stop { .. } => ("Stop", String::new()),
            HookEvent::SubagentStop { .. } => ("SubagentStop", String::new()),
        };
        if ev_name != self.event_type {
            return false;
        }
        match &self.matcher {
            Some(r) => r.is_match(&target),
            None => true,
        }
    }

    async fn run(&self, event: &HookEvent) -> anyhow::Result<HookResponse> {
        use tokio::io::AsyncWriteExt;
        let payload = serde_json::to_string(event)?;

        let mut child = tokio::process::Command::new("bash")
            .arg("-c")
            .arg(&self.command)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(payload.as_bytes()).await?;
            stdin.shutdown().await?;
        }

        let out = tokio::time::timeout(self.timeout, child.wait_with_output()).await??;
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();

        if !out.status.success() {
            return Ok(HookResponse {
                block: true,
                reason: Some(format!(
                    "hook {} failed: {}",
                    self.id,
                    String::from_utf8_lossy(&out.stderr)
                )),
                ..Default::default()
            });
        }
        if stdout.trim().is_empty() {
            return Ok(HookResponse::default());
        }
        Ok(serde_json::from_str(&stdout).unwrap_or_default())
    }
}

pub struct AutoFormatHook;

#[async_trait]
impl Hook for AutoFormatHook {
    fn id(&self) -> &str { "auto_format" }

    fn matches(&self, event: &HookEvent) -> bool {
        matches!(
            event,
            HookEvent::PostToolUse { tool_name, .. } if tool_name == "write_file"
        )
    }

    async fn run(&self, event: &HookEvent) -> anyhow::Result<HookResponse> {
        if let HookEvent::PostToolUse { output, .. } = event {
            if let Some(path) = output.strip_prefix("written: ") {
                let path = path.split_whitespace().next().unwrap_or("");
                if path.ends_with(".rs") {
                    let _ = tokio::process::Command::new("rustfmt").arg(path).output().await;
                }
            }
        }
        Ok(HookResponse::default())
    }
}

pub struct HookDispatcher {
    pub hooks: Vec<Arc<dyn Hook>>,
}

impl HookDispatcher {
    pub async fn dispatch(&self, event: HookEvent) -> HookResponse {
        let mut merged = HookResponse::default();
        for h in &self.hooks {
            if !h.matches(&event) { continue; }
            match h.run(&event).await {
                Ok(r) => {
                    if r.block { return r; }
                    if r.inject.is_some() { merged.inject = r.inject; }
                    if r.replace_output.is_some() { merged.replace_output = r.replace_output; }
                }
                Err(e) => tracing::error!(hook = h.id(), error = %e, "hook failed"),
            }
        }
        merged
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_env_filter("info").init();

    let dispatcher = HookDispatcher {
        hooks: vec![
            Arc::new(ShellHook {
                id: "audit".into(),
                event_type: "PreToolUse".into(),
                matcher: Some(regex::Regex::new(r"^run_bash\(.*git ")?),
                command: r#"cat && echo '{}'"#.into(),
                timeout: std::time::Duration::from_secs(5),
            }),
            Arc::new(AutoFormatHook),
        ],
    };

    let resp = dispatcher.dispatch(HookEvent::PreToolUse {
        session_id: "demo".into(),
        tool_name: "run_bash".into(),
        input: serde_json::json!({"command": "git status"}),
    }).await;

    println!("dispatch result: {resp:?}");
    Ok(())
}
