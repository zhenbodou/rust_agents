# 第 25 章 Session 与持久化记忆

> Agent 能记住"上次我们做了什么"——这一章让它具备这个能力。

## 25.1 Session 与 Memory 的区别

| | Session | Memory |
|---|---|---|
| 生命周期 | 一次对话 | 跨对话持久 |
| 粒度 | 细（每轮都存） | 粗（只存关键事实） |
| 用途 | 回放 / 恢复 / 审计 | 下一次对话有"先验" |
| 格式 | JSONL 流 | Markdown 文件 |

## 25.2 Session 存储布局

```
~/.mcc/sessions/
└── 2026-04-23/
    ├── 1a2b3c.jsonl         # turn 流水
    ├── 1a2b3c.meta.json     # 元数据：开始时间、cwd、cost、status
    └── 1a2b3c.messages.json # 末态 messages 快照（便于 resume）
```

## 25.3 SessionRecorder

```rust
pub struct SessionRecorder {
    pub id: String,
    writer: tokio::sync::Mutex<tokio::fs::File>,
    meta_path: PathBuf,
    meta: Mutex<SessionMeta>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMeta {
    pub id: String,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub cwd: PathBuf,
    pub turns: u32,
    pub cost_usd: f64,
    pub status: String,   // running | ended | errored
    pub title: Option<String>,   // 由前 2 轮对话生成，便于列表
}

impl SessionRecorder {
    pub async fn open(id: &str, cwd: &Path) -> anyhow::Result<Self> {
        let home = dirs::home_dir().unwrap().join(".mcc/sessions");
        let date = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let dir = home.join(date);
        tokio::fs::create_dir_all(&dir).await?;

        let jsonl = dir.join(format!("{id}.jsonl"));
        let meta_path = dir.join(format!("{id}.meta.json"));

        let file = tokio::fs::OpenOptions::new().create(true).append(true).open(&jsonl).await?;
        let meta = SessionMeta {
            id: id.into(),
            started_at: chrono::Utc::now(),
            cwd: cwd.into(),
            turns: 0,
            cost_usd: 0.0,
            status: "running".into(),
            title: None,
        };
        tokio::fs::write(&meta_path, serde_json::to_string_pretty(&meta)?).await?;
        Ok(Self { id: id.into(), writer: Mutex::new(file), meta_path, meta: Mutex::new(meta) })
    }

    pub async fn record(&self, turn: TurnSnapshot) -> anyhow::Result<()> {
        use tokio::io::AsyncWriteExt;
        let line = serde_json::to_string(&turn)? + "\n";
        self.writer.lock().await.write_all(line.as_bytes()).await?;

        let mut m = self.meta.lock().unwrap();
        m.turns += 1;
        m.cost_usd += estimate_cost_of(&turn.usage);
        let snapshot = m.clone();
        drop(m);
        tokio::fs::write(&self.meta_path, serde_json::to_string_pretty(&snapshot)?).await?;
        Ok(())
    }

    pub async fn finalize(&self, status: &str) -> anyhow::Result<()> {
        let mut m = self.meta.lock().unwrap();
        m.status = status.into();
        let snapshot = m.clone();
        drop(m);
        tokio::fs::write(&self.meta_path, serde_json::to_string_pretty(&snapshot)?).await?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnSnapshot {
    pub ts: chrono::DateTime<chrono::Utc>,
    pub iteration: u32,
    pub request_messages: Vec<Message>,
    pub assistant_blocks: Vec<ContentBlock>,
    pub tool_outputs: Vec<(String, String, bool)>,
    pub usage: Usage,
    pub model: String,
}
```

## 25.4 Resume：恢复上次对话

```rust
pub async fn load_session(id: &str) -> anyhow::Result<(SessionMeta, Vec<Message>)> {
    let sessions_dir = dirs::home_dir().unwrap().join(".mcc/sessions");

    // 按日期目录扫描
    let mut meta_path = None;
    let mut rd = tokio::fs::read_dir(&sessions_dir).await?;
    while let Some(day) = rd.next_entry().await? {
        let candidate = day.path().join(format!("{id}.meta.json"));
        if candidate.exists() { meta_path = Some(candidate); break; }
    }
    let meta_path = meta_path.ok_or_else(|| anyhow::anyhow!("session not found"))?;
    let meta: SessionMeta = serde_json::from_slice(&tokio::fs::read(&meta_path).await?)?;

    // 从 jsonl 重建 messages
    let jsonl = meta_path.with_extension("jsonl").with_extension("jsonl");
    let raw = tokio::fs::read_to_string(meta_path.with_extension("").with_extension("jsonl")).await?;
    let mut messages = Vec::new();
    for line in raw.lines() {
        let turn: TurnSnapshot = serde_json::from_str(line)?;
        messages = turn.request_messages;  // 每一行都有完整 messages，取最后一个
        messages.push(Message { role: Role::Assistant, content: turn.assistant_blocks });
        if !turn.tool_outputs.is_empty() {
            let blocks = turn.tool_outputs.into_iter().map(|(id, out, err)| ContentBlock::ToolResult {
                tool_use_id: id, content: out, is_error: err,
            }).collect();
            messages.push(Message { role: Role::User, content: blocks });
        }
    }
    Ok((meta, messages))
}
```

CLI 入口：

```bash
mcc resume 1a2b3c
```

## 25.5 Title 自动生成

第一轮对话结束后，派一个 Haiku 总结对话主题作为 title，给 `mcc sessions list` 显示：

```rust
pub async fn maybe_generate_title(meta: &mut SessionMeta, llm: &dyn LlmProvider, first_turn: &TurnSnapshot) -> anyhow::Result<()> {
    if meta.title.is_some() || meta.turns < 1 { return Ok(()); }
    let first_user = first_turn.request_messages.iter().filter_map(|m| {
        if matches!(m.role, Role::User) {
            m.content.iter().find_map(|b| if let ContentBlock::Text { text, .. } = b { Some(text.clone()) } else { None })
        } else { None }
    }).next().unwrap_or_default();

    let resp = llm.complete(CompleteRequest {
        model: "claude-haiku-4-5-20251001".into(),
        max_tokens: 32,
        messages: vec![Message::user(format!("用 10 个中文字以内概括这次对话主题：\n{first_user}"))],
        system: None, temperature: Some(0.0), tools: None,
    }).await?;
    if let Some(text) = resp.content.iter().find_map(|b| if let ContentBlock::Text { text, .. } = b { Some(text.clone()) } else { None }) {
        meta.title = Some(text.trim().to_string());
    }
    Ok(())
}
```

## 25.6 Sessions 列表

```rust
pub async fn list_sessions() -> anyhow::Result<Vec<SessionMeta>> {
    let dir = dirs::home_dir().unwrap().join(".mcc/sessions");
    let mut metas = Vec::new();
    let mut rd = tokio::fs::read_dir(&dir).await?;
    while let Some(day) = rd.next_entry().await? {
        if !day.file_type().await?.is_dir() { continue; }
        let mut rd2 = tokio::fs::read_dir(day.path()).await?;
        while let Some(f) = rd2.next_entry().await? {
            if f.file_name().to_string_lossy().ends_with(".meta.json") {
                if let Ok(raw) = tokio::fs::read(f.path()).await {
                    if let Ok(m) = serde_json::from_slice::<SessionMeta>(&raw) {
                        metas.push(m);
                    }
                }
            }
        }
    }
    metas.sort_by(|a, b| b.started_at.cmp(&a.started_at));
    Ok(metas)
}
```

CLI 输出（`mcc sessions list`）：

```
ID       STARTED             TURNS  COST    TITLE
1a2b3c   2026-04-22 14:32    8      $0.34   修复 rpc client 超时
8f9d2e   2026-04-22 11:05    3      $0.09   添加 grpc 健康检查
4c1a77   2026-04-20 19:21    22     $1.02   rust async trait 重构
```

## 25.7 长期记忆接入

复用第 8 章的 `MarkdownMemoryStore`。两个动作：

1. **启动时注入索引**：把 `~/.mcc/memory/MEMORY.md` 内容加到 system prompt 的 `# Memory Index` 段
2. **暴露 tool**：

```rust
pub struct SaveMemoryTool { store: Arc<MarkdownMemoryStore> }
pub struct ReadMemoryTool { store: Arc<MarkdownMemoryStore> }
```

示例 schema：

```rust
fn input_schema(&self) -> Value {
    json!({
        "type":"object","required":["name","type","description","body"],
        "properties":{
            "name":{"type":"string","description":"Stable filename-safe id, e.g. 'user_role'"},
            "type":{"enum":["user","project","feedback","reference"]},
            "description":{"type":"string","description":"One line describing when to use"},
            "body":{"type":"string","description":"The actual memory content (markdown)"},
            "tags":{"type":"array","items":{"type":"string"}}
        }
    })
}
```

Agent 在 system 里被指引：用户告诉我偏好 → 调用 `save_memory`；记忆命中相关 → 调用 `read_memory` 获取详情。

## 25.8 隐私与安全

- Session 里可能有完整文件内容，**默认加密**可选：用 `age` 或 `chacha20poly1305` 对 jsonl 加密
- `mcc sessions purge --older-than 30d` 定期清理
- 企业部署：session 存到远端对象存储 + 访问审计

## 25.9 小结

- JSONL 是 session 的最佳格式：边写边落、可 grep、可 replay
- Title 让多 session 可管理
- Memory 作为 tool 暴露给 Agent，让它自主维护
- 隐私与清理策略同样是产品的一部分

> **下一章**：Subagent 并行 —— 让主 Agent 委派任务。

