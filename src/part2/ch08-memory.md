# 第 8 章 记忆系统：短期 / 长期 / 向量检索

> 目标：理解 Agent 记忆的三层结构，并用 Rust 实现一套**可落地的生产级**方案。

## 8.1 三层记忆模型

```text
┌────────────────────────────────────────────────┐
│ 短期记忆 (Working Memory) = 当前 messages       │
│   特点：token 限制强，每轮都在变                │
│   技术：消息列表 + 滑动窗口 + 自动摘要           │
├────────────────────────────────────────────────┤
│ 长期记忆 (Long-term) = 跨会话事实               │
│   特点：用户偏好、项目知识、历史决策             │
│   技术：键值存储 / 结构化文件 / 数据库          │
├────────────────────────────────────────────────┤
│ 向量记忆 (Semantic) = 可检索的语义 chunks       │
│   特点：大量非结构化资料                         │
│   技术：embedding + 向量库 + RAG                │
└────────────────────────────────────────────────┘
```

**现实建议**：别一上来就上 RAG。**先用好前两层，95% 场景够用。**

## 8.2 短期记忆：滑动窗口 + 摘要压缩

```rust
pub struct ShortTermMemory {
    messages: Vec<Message>,
    max_tokens: usize,
    summarizer: Arc<dyn LlmProvider>,
    model: String,
}

impl ShortTermMemory {
    pub async fn add_and_compact(&mut self, msg: Message) -> anyhow::Result<()> {
        self.messages.push(msg);
        if self.estimated_tokens() > self.max_tokens {
            self.compact().await?;
        }
        Ok(())
    }

    fn estimated_tokens(&self) -> usize {
        // 粗略：字符数 / 3.5。生产环境用 tiktoken / Anthropic count_tokens API
        self.messages.iter().map(|m| {
            m.content.iter().map(|b| match b {
                ContentBlock::Text { text } => text.len(),
                ContentBlock::ToolUse { input, .. } => input.to_string().len(),
                ContentBlock::ToolResult { content, .. } => content.len(),
            }).sum::<usize>()
        }).sum::<usize>() / 3
    }

    async fn compact(&mut self) -> anyhow::Result<()> {
        // 保留最近 N 条原文，把更早的压缩成一段摘要
        let keep_recent = 8;
        if self.messages.len() <= keep_recent { return Ok(()) }

        let (old, new) = self.messages.split_at(self.messages.len() - keep_recent);
        let old_dump = serde_json::to_string(&old)?;

        let resp = self.summarizer.complete(CompleteRequest {
            model: self.model.clone(),
            max_tokens: 1024,
            messages: vec![Message::user(format!(
                "以下是 Agent 前期对话与工具调用记录。请压缩为不超过 500 字的结构化摘要，\
                 包含：用户目标、已完成的工具调用与关键发现、待解决问题、已验证的事实。\n\n<history>\n{old_dump}\n</history>"
            ))],
            system: Some("你是一个擅长压缩上下文的助理。".into()),
            temperature: Some(0.0),
            tools: None,
        }).await?;

        let summary = resp.content.iter().filter_map(|b| {
            if let ContentBlock::Text { text } = b { Some(text.as_str()) } else { None }
        }).collect::<Vec<_>>().join("\n");

        let compacted = Message {
            role: Role::User,
            content: vec![ContentBlock::Text { text: format!("<prior_session_summary>\n{summary}\n</prior_session_summary>") }],
        };

        self.messages = std::iter::once(compacted).chain(new.iter().cloned()).collect();
        Ok(())
    }
}
```

**工程要点**：
- 摘要用**便宜模型**（Haiku），不占主 Agent 预算
- 保留最近 N 轮原文——近期细节丢失代价最大
- 摘要本身也带结构化字段（目标/发现/待办），方便后续精确引用

## 8.3 长期记忆：一个"可写的 Markdown 仓库"

Claude Code 内置的 memory 系统就是这个思路——**不要数据库，就用文件**。优点：

- 人类可读，用户可以直接编辑
- 版本化简单（放 git）
- Debug 容易
- 迁移到新项目 / 新机器零成本

### Rust 实现：`MarkdownMemoryStore`

```rust
use std::path::PathBuf;
use tokio::fs;

pub struct MarkdownMemoryStore {
    root: PathBuf,     // e.g. ~/.myagent/memory/
}

impl MarkdownMemoryStore {
    pub async fn save(&self, name: &str, content: &MemoryEntry) -> anyhow::Result<()> {
        fs::create_dir_all(&self.root).await?;
        let file = self.root.join(format!("{}.md", sanitize(name)));
        let body = format!(
            "---\nname: {}\ntype: {}\ntags: [{}]\nupdated: {}\n---\n\n{}\n",
            content.name,
            content.kind,
            content.tags.join(", "),
            chrono::Utc::now().to_rfc3339(),
            content.body
        );
        fs::write(file, body).await?;
        self.update_index().await?;
        Ok(())
    }

    pub async fn load_all(&self) -> anyhow::Result<Vec<MemoryEntry>> {
        let mut out = Vec::new();
        let mut rd = fs::read_dir(&self.root).await?;
        while let Some(e) = rd.next_entry().await? {
            if e.path().extension().map(|x| x == "md").unwrap_or(false)
                && e.file_name() != "MEMORY.md"
            {
                let raw = fs::read_to_string(e.path()).await?;
                if let Some(entry) = parse_entry(&raw) { out.push(entry); }
            }
        }
        Ok(out)
    }

    /// 给 LLM 的"入口索引"——永远塞进 system prompt
    pub async fn index_for_prompt(&self) -> anyhow::Result<String> {
        let path = self.root.join("MEMORY.md");
        if !path.exists() { return Ok(String::new()); }
        Ok(fs::read_to_string(path).await?)
    }

    async fn update_index(&self) -> anyhow::Result<()> {
        let entries = self.load_all().await?;
        let mut idx = String::from("# Memory Index\n\n");
        for e in entries {
            idx.push_str(&format!("- [{}]({}.md) — {}\n", e.name, sanitize(&e.name), e.description));
        }
        fs::write(self.root.join("MEMORY.md"), idx).await?;
        Ok(())
    }
}

pub struct MemoryEntry {
    pub name: String,
    pub kind: String,        // "user" | "project" | "feedback" | "reference"
    pub tags: Vec<String>,
    pub description: String,
    pub body: String,
}

fn sanitize(name: &str) -> String {
    name.chars().map(|c| if c.is_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).collect()
}

fn parse_entry(raw: &str) -> Option<MemoryEntry> {
    // 简化：生产用 gray_matter crate
    let parts: Vec<&str> = raw.splitn(3, "---").collect();
    if parts.len() < 3 { return None; }
    let fm = parts[1];
    let body = parts[2].trim().to_string();
    let mut e = MemoryEntry { name: String::new(), kind: String::new(), tags: vec![], description: String::new(), body };
    for line in fm.lines() {
        if let Some((k, v)) = line.split_once(':') {
            match k.trim() {
                "name" => e.name = v.trim().into(),
                "type" => e.kind = v.trim().into(),
                "description" => e.description = v.trim().into(),
                _ => {}
            }
        }
    }
    Some(e)
}
```

### 怎么让 Agent 主动用这个系统？

**把它做成两个 tool**：`save_memory(name, type, content)` 和 `read_memory(name)`。再把 `MEMORY.md` 作为 system prompt 的一部分自动注入，让模型知道"当前已有哪些记忆可查"。

这正是 Claude Code 的做法。第 25 章实战里我们会把它接入 mini-claude-code。

## 8.4 向量记忆：最简 RAG

**什么时候需要 RAG**：你有 > 100 个文档，或文档总量超过上下文窗口，或有高度非结构化资料（论文、工单、聊天记录）。

### 8.4.1 Embedding API

Anthropic 目前没有官方 embedding，生产常用 OpenAI 或开源模型。用 OpenAI 兼容接口：

```rust
pub struct OpenAiEmbedder { http: Client, api_key: String, model: String }

impl OpenAiEmbedder {
    pub async fn embed(&self, texts: &[String]) -> anyhow::Result<Vec<Vec<f32>>> {
        let resp = self.http
            .post("https://api.openai.com/v1/embeddings")
            .bearer_auth(&self.api_key)
            .json(&serde_json::json!({"model": self.model, "input": texts}))
            .send().await?.error_for_status()?
            .json::<serde_json::Value>().await?;

        let data = resp["data"].as_array().ok_or_else(|| anyhow::anyhow!("no data"))?;
        Ok(data.iter().map(|d| {
            d["embedding"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap() as f32).collect()
        }).collect())
    }
}
```

### 8.4.2 本地向量库：用 `hnsw_rs` 或 Qdrant

轻量级生产方案：嵌入式向量库 [`qdrant-client`](https://crates.io/crates/qdrant-client)（需跑 Qdrant 服务）。零依赖做法：

```rust
use hnsw_rs::prelude::*;

pub struct VectorStore {
    hnsw: Hnsw<'static, f32, DistCosine>,
    chunks: Vec<Chunk>,
}
pub struct Chunk { pub id: usize, pub text: String, pub source: String }

impl VectorStore {
    pub fn new() -> Self {
        let hnsw = Hnsw::new(16, 10_000, 16, 200, DistCosine {});
        Self { hnsw, chunks: vec![] }
    }

    pub fn add(&mut self, embedding: Vec<f32>, chunk: Chunk) {
        let id = self.chunks.len();
        self.hnsw.insert((&embedding, id));
        self.chunks.push(Chunk { id, ..chunk });
    }

    pub fn search(&self, query: &[f32], k: usize) -> Vec<(f32, &Chunk)> {
        self.hnsw.search(query, k, 64).into_iter()
            .map(|n| (n.distance, &self.chunks[n.d_id])).collect()
    }
}
```

### 8.4.3 Chunking 策略

好的 chunk 决定检索质量：

- **代码**：按函数/类切，`tree-sitter` 最佳
- **Markdown**：按标题层级切
- **普通文本**：按段落 + overlap 100 token
- 每个 chunk 附加"来源"元数据（文件名、行号、章节）

### 8.4.4 作为 Tool 暴露给 Agent

```rust
pub struct SearchKnowledgeTool { store: Arc<RwLock<VectorStore>>, embedder: Arc<OpenAiEmbedder> }

#[async_trait]
impl Tool for SearchKnowledgeTool {
    fn name(&self) -> &str { "search_knowledge" }
    fn description(&self) -> &str { "Search internal knowledge base by semantic similarity. Returns top 5 chunks." }
    fn input_schema(&self) -> Value {
        json!({"type":"object","required":["query"],
               "properties":{"query":{"type":"string"}, "k":{"type":"integer","default":5}}})
    }
    async fn execute(&self, input: Value, _: &ToolContext) -> ToolOutput {
        #[derive(serde::Deserialize)] struct A { query: String, #[serde(default="k5")] k: usize }
        fn k5() -> usize { 5 }
        let a: A = match serde_json::from_value(input) { Ok(a)=>a,Err(e)=>return ToolOutput::err(e.to_string()) };
        let emb = match self.embedder.embed(&[a.query.clone()]).await {
            Ok(mut v) => v.remove(0), Err(e) => return ToolOutput::err(e.to_string()),
        };
        let store = self.store.read().await;
        let hits = store.search(&emb, a.k);
        let mut out = String::new();
        for (d, c) in hits {
            out.push_str(&format!("[score={:.3}] ({})\n{}\n---\n", 1.0-d, c.source, c.text));
        }
        ToolOutput::ok(out)
    }
}
```

## 8.5 设计决策：什么该记，什么不该记

这是工业经验（严格模仿 Claude Code 的原则）：

**应该记**：
- 用户偏好、角色、技术栈
- 非显而易见的项目决策（为什么这么选）
- 外部系统引用（监控板 URL、工单系统项目名）
- 反馈与修正（不要做什么）

**不该记**：
- 代码结构、函数签名（读代码就有）
- Git history（用 git log）
- 一次性调试结论
- 当前会话的临时状态（用 session 而非 memory）

## 8.6 小结

- 三层记忆：短期（压缩）、长期（Markdown）、向量（RAG）
- **优先长期 Markdown 记忆**，可读、可 diff、可 git
- 向量库只在必要时上，chunk 质量比库选型更重要

> 🎉 **Part 2 完结**：你已经具备构建完整 Agent 的能力。接下来 Part 3 进入 Harness Engineering —— 让 Agent 变成**产品**的那层功夫。

