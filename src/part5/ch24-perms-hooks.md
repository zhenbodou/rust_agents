# 第 24 章 权限与 Hooks 实现

> 把第 11–12 章的抽象接入 mini-claude-code，变成真正可用的子系统。

## 24.1 配置加载

`crates/mcc-config/src/lib.rs`：

```rust
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub model: ModelConfig,
    #[serde(default)]
    pub permissions: PermissionConfig,
    #[serde(default)]
    pub hooks: HooksConfig,
    #[serde(default)]
    pub observability: ObservabilityConfig,
    #[serde(default)]
    pub budget: BudgetConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig { pub main: String, pub subagent: String, pub summarize: String }
impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            main: "claude-opus-4-7".into(),
            subagent: "claude-sonnet-4-6".into(),
            summarize: "claude-haiku-4-5-20251001".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HooksConfig {
    #[serde(default)] pub PreToolUse: Vec<HookRule>,
    #[serde(default)] pub PostToolUse: Vec<HookRule>,
    #[serde(default)] pub UserPromptSubmit: Vec<HookRule>,
    #[serde(default)] pub Stop: Vec<HookRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookRule { pub matcher: Option<String>, pub command: String, #[serde(default="default_timeout")] pub timeout_sec: u64 }
fn default_timeout() -> u64 { 30 }

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ObservabilityConfig { pub log_format: Option<String>, pub otel_endpoint: Option<String> }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetConfig { pub max_usd_per_session: f64, pub max_iterations: u32 }
impl Default for BudgetConfig { fn default() -> Self { Self { max_usd_per_session: 2.0, max_iterations: 40 } } }

pub async fn load(cwd: &Path) -> anyhow::Result<Config> {
    let home = home_config().await;
    let project = project_config(cwd).await;
    Ok(merge(home, project))
}

async fn home_config() -> Option<Config> {
    let home = dirs::home_dir()?;
    let p = home.join(".mcc/settings.json");
    if !p.exists() { return None; }
    let raw = tokio::fs::read_to_string(&p).await.ok()?;
    serde_json::from_str(&raw).ok()
}
async fn project_config(cwd: &Path) -> Option<Config> {
    let p = cwd.join(".mcc/settings.json");
    if !p.exists() { return None; }
    let raw = tokio::fs::read_to_string(&p).await.ok()?;
    serde_json::from_str(&raw).ok()
}

fn merge(home: Option<Config>, proj: Option<Config>) -> Config {
    let mut base = home.unwrap_or_default();
    if let Some(p) = proj {
        base.permissions.deny.extend(p.permissions.deny);
        base.permissions.allow.extend(p.permissions.allow);
        if p.permissions.mode.is_some() { base.permissions.mode = p.permissions.mode; }
        base.hooks.PreToolUse.extend(p.hooks.PreToolUse);
        base.hooks.PostToolUse.extend(p.hooks.PostToolUse);
        base.hooks.UserPromptSubmit.extend(p.hooks.UserPromptSubmit);
        base.hooks.Stop.extend(p.hooks.Stop);
        if p.model.main != base.model.main { base.model = p.model; }
        base.budget = p.budget;
    }
    base
}
```

**合并策略**（重要的工业细节）：

- `deny` 取并集（项目永远只能**加强**限制）
- `allow` 取并集
- 模式：项目覆盖
- 数值：项目覆盖

## 24.2 UserPrompter 实现

CLI 模式用 stdin 提问；TUI 模式弹模态框。

```rust
#[async_trait::async_trait]
pub trait UserPrompter: Send + Sync {
    async fn ask(&self, msg: &str) -> bool;
}

/// CLI 模式
pub struct StdioPrompter;

#[async_trait::async_trait]
impl UserPrompter for StdioPrompter {
    async fn ask(&self, msg: &str) -> bool {
        use std::io::Write;
        print!("\n⚠ {msg}\n[y/N] ");
        std::io::stdout().flush().ok();
        let mut buf = String::new();
        std::io::stdin().read_line(&mut buf).ok();
        matches!(buf.trim().to_ascii_lowercase().as_str(), "y" | "yes")
    }
}

/// TUI 模式：发事件给 UI，等用户选择
pub struct TuiPrompter {
    pub tx: tokio::sync::mpsc::UnboundedSender<AgentEvent>,
    pub answers: Arc<Mutex<HashMap<u64, tokio::sync::oneshot::Sender<bool>>>>,
    pub next_id: Arc<AtomicU64>,
}

#[async_trait::async_trait]
impl UserPrompter for TuiPrompter {
    async fn ask(&self, msg: &str) -> bool {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.answers.lock().unwrap().insert(id, tx);
        let _ = self.tx.send(AgentEvent::PermissionRequest { id, message: msg.into() });
        rx.await.unwrap_or(false)
    }
}
```

TUI 收到 `PermissionRequest` 事件时弹浮窗，用户按 y/n 后通过 `session.resolve_permission(id, answer)` 回写 `answers`。

## 24.3 Hook Dispatcher 接入配置

```rust
pub fn build_dispatcher(cfg: &HooksConfig) -> HookDispatcher {
    let mut hooks: Vec<Arc<dyn Hook>> = Vec::new();

    for (ev, rules) in [
        ("PreToolUse", &cfg.PreToolUse),
        ("PostToolUse", &cfg.PostToolUse),
        ("UserPromptSubmit", &cfg.UserPromptSubmit),
        ("Stop", &cfg.Stop),
    ] {
        for (i, rule) in rules.iter().enumerate() {
            let matcher = rule.matcher.as_deref().and_then(|m| regex::Regex::new(m).ok());
            hooks.push(Arc::new(ShellHook {
                id: format!("{ev}#{i}"),
                event_type: ev.to_string(),
                matcher,
                command: rule.command.clone(),
                timeout: std::time::Duration::from_secs(rule.timeout_sec),
            }));
        }
    }
    // 内建 Rust hooks
    hooks.push(Arc::new(AutoFormatHook));

    HookDispatcher { hooks }
}
```

## 24.4 示例 Hook 脚本（随项目分发）

`hooks/audit-git.sh`：

```bash
#!/usr/bin/env bash
payload=$(cat)
cmd=$(echo "$payload" | jq -r '.input.command')
# 记录所有 git 命令
mkdir -p .mcc/audit
echo "[$(date -Iseconds)] $cmd" >> .mcc/audit/git.log

# push --force 直接拒
if echo "$cmd" | grep -qE '^git push .*--force'; then
  jq -n '{block:true, reason:"force push requires manual confirmation outside mcc"}'
  exit 0
fi
echo '{}'
```

`hooks/deny-secrets-read.sh`：

```bash
#!/usr/bin/env bash
payload=$(cat)
path=$(echo "$payload" | jq -r '.input.path // ""')
if echo "$path" | grep -qE '\.(env|pem)$|id_rsa|\.aws/credentials'; then
  jq -n --arg p "$path" '{block:true, reason:("secrets access denied: " + $p)}'
else
  echo '{}'
fi
```

对应 `.mcc/settings.json`：

```json
{
  "hooks": {
    "PreToolUse": [
      { "matcher": "Bash\\(git ", "command": "bash .mcc/hooks/audit-git.sh" },
      { "matcher": "Read\\(.+\\)|write_file|edit_file", "command": "bash .mcc/hooks/deny-secrets-read.sh" }
    ]
  }
}
```

## 24.5 组装 Agent

`mcc-harness::build_agent`：

```rust
pub async fn build_agent(cfg: &Config, cwd: PathBuf) -> anyhow::Result<ProductionAgent> {
    let llm: Arc<dyn LlmProvider> = Arc::new(
        ThrottledLlm::new(Arc::new(AnthropicClient::from_env()?), 50, 4)
    );
    let perms = Arc::new(PermissionChecker::new(&cfg.permissions)?);
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let prompter = Arc::new(StdioPrompter);
    let registry = Arc::new(build_registry(perms.clone(), prompter.clone(), tx.clone()));
    let dispatcher = Arc::new(build_dispatcher(&cfg.hooks));

    let session_id = uuid::Uuid::new_v4().to_string();
    let ctx = ToolContext { cwd: cwd.clone(), session_id: session_id.clone() };

    let system = build_system_prompt(&EnvInfo::collect(&cwd), &load_memory_index(&cwd).await, &[]);

    Ok(ProductionAgent {
        llm,
        registry,
        ctx,
        system,
        model: cfg.model.main.clone(),
        config: AgentConfig {
            max_iterations: cfg.budget.max_iterations,
            max_tokens_per_call: 8192,
            budget_usd: cfg.budget.max_usd_per_session,
            temperature: 0.0,
            retry: RetryPolicy { max_attempts: 3, base: Duration::from_millis(500), cap: Duration::from_secs(30) },
        },
        recorder: Arc::new(SessionRecorder::open(&session_id).await?),
        dispatcher,
        event_tx: tx,
        cancel: tokio_util::sync::CancellationToken::new(),
        cost: Arc::new(Mutex::new(CostTracker::default())),
    })
}
```

## 24.6 测试权限阻断

```rust
#[tokio::test]
async fn bash_rm_rf_is_denied_by_default() {
    let cfg = PermissionConfig {
        deny: vec!["Bash(rm -rf *)".into()], ..Default::default()
    };
    let checker = PermissionChecker::new(&cfg).unwrap();
    let req = PermissionRequest { category:"Bash".into(), action: Action::Bash { cmd: "rm -rf /tmp/test".into() } };
    assert!(matches!(checker.check(&req), Decision::Deny(_)));
}
```

## 24.7 小结

- 配置合并：home → project，deny 取并集
- Hook 分 shell + 内建 Rust 两类
- Prompter 抽象让 CLI / TUI 都能询问
- 示例脚本即是团队规范落地

> **下一章**：Session 持久化 + 可恢复 + 长期记忆接入。

