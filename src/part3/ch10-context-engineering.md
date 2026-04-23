# 第 10 章 Context Engineering：上下文即产品

> "模型是杠杆，上下文是支点。" —— 一位 Anthropic 工程师在 podcast 里的原话。

## 10.1 为什么是"上下文工程"而不是"Prompt 工程"

Prompt 工程是**单次**的、**静态**的措辞优化。
Context Engineering 是**每一次 API 调用动态构造上下文**的系统工程：

- 放什么（哪些文件、哪些记忆）
- 不放什么（过时的、敏感的、太长的）
- 什么顺序（缓存命中、首尾位置）
- 怎么压缩（摘要、截断、chunk）

**一个优秀的 Agent 产品，80% 的"聪明感"来自上下文工程。**

## 10.2 Context 的六层结构（Anthropic 风格）

从 API 角度，一个 Agent 请求的上下文大致长这样：

```text
messages[]:
┌───────────────────────────────────────────────────┐
│ system  ←  角色 + 指令 + 工具使用策略             │
│   + 环境信息 (cwd, platform, date)                │
│   + MEMORY.md 索引                                │
│   + 核心 workflow 约定                            │
├───────────────────────────────────────────────────┤
│ user: <ide_context> 打开的文件、选中代码 </>      │
│ user: <project_context> CLAUDE.md, 主要文件 </>   │
│ ── 以上通常命中 prompt cache ──                   │
├───────────────────────────────────────────────────┤
│ user: 本次真正的问题                              │
│ assistant: …                                       │
│ user: <tool_result>…</tool_result>                │
│ …                                                 │
└───────────────────────────────────────────────────┘
```

**缓存边界**在中间——前面是稳定的"产品上下文"，后面是多变的对话。这是第 16 章讲 cache 的伏笔。

## 10.3 System Prompt 的黄金结构

参考 Claude Code 系统提示词的结构（社区逆向版本已多次公开讨论）：

```markdown
# Identity
你是 MyAgent，一位用 Rust 写代码的资深助手……

# Capabilities
- 你可以读写文件、执行 shell、搜索代码
- 你 **不能** 访问互联网（除非提供 web tool）

# Safety
- 不要在未确认前运行破坏性命令
- 处理敏感文件时先检查 .gitignore

# Communication style
- 中文回复，代码块用 fenced code
- 保持简洁，先说结论再说理由

# Environment
- 平台: {{platform}}
- 工作目录: {{cwd}}
- Git: {{is_git_repo}}
- 当前日期: {{today}}

# Memory Index
{{memory_index_md}}

# Workflows
{{available_slash_commands}}
```

### Rust 模板引擎落地

```rust
use tera::{Tera, Context};
use once_cell::sync::Lazy;

static TPL: Lazy<Tera> = Lazy::new(|| {
    let mut t = Tera::default();
    t.add_raw_template("system", include_str!("../prompts/system.md")).unwrap();
    t
});

pub fn build_system_prompt(env: &EnvInfo, memory_idx: &str, skills: &[String]) -> String {
    let mut c = Context::new();
    c.insert("platform", &env.platform);
    c.insert("cwd", &env.cwd);
    c.insert("is_git_repo", &env.is_git);
    c.insert("today", &chrono::Utc::now().format("%Y-%m-%d").to_string());
    c.insert("memory_index_md", memory_idx);
    c.insert("available_slash_commands", skills);
    TPL.render("system", &c).expect("template")
}
```

## 10.4 动态上下文注入的四种时机

| 时机 | 做什么 | 例子 |
|---|---|---|
| **Session start** | 注入长期事实 | CLAUDE.md、memory index |
| **Per-turn** | 注入变化信息 | 当前 git status、IDE selection |
| **On-demand** | 用 tool 拉取 | 读某个文件 |
| **Reflex** | 某事件触发自动注入 | 用户 @提及 issue → 拉 issue 内容 |

Rust 里我们用 **Context Builder 模式** 组装：

```rust
pub trait ContextProvider: Send + Sync {
    fn id(&self) -> &str;
    async fn provide(&self, req: &TurnRequest) -> anyhow::Result<Option<ContextFragment>>;
}

pub struct ContextFragment {
    pub tag: String,            // <project_context>, <git_status>, ...
    pub body: String,
    pub cacheable: bool,
    pub priority: u32,          // 决定拼接顺序
}

pub struct ContextBuilder {
    providers: Vec<Arc<dyn ContextProvider>>,
}

impl ContextBuilder {
    pub async fn build(&self, req: &TurnRequest) -> anyhow::Result<Vec<Message>> {
        let mut fragments = Vec::new();
        for p in &self.providers {
            if let Some(f) = p.provide(req).await? { fragments.push(f); }
        }
        fragments.sort_by_key(|f| f.priority);

        let (cacheable, volatile): (Vec<_>, Vec<_>) = fragments.into_iter().partition(|f| f.cacheable);

        let mut msgs = Vec::new();
        if !cacheable.is_empty() {
            let body = cacheable.iter().map(|f| format!("<{tag}>\n{body}\n</{tag}>", tag=f.tag, body=f.body))
                .collect::<Vec<_>>().join("\n\n");
            // 后面 cache 章节会把这块标为 cache_control
            msgs.push(Message::user(body));
        }
        if !volatile.is_empty() {
            let body = volatile.iter().map(|f| format!("<{tag}>\n{body}\n</{tag}>", tag=f.tag, body=f.body))
                .collect::<Vec<_>>().join("\n\n");
            msgs.push(Message::user(body));
        }
        Ok(msgs)
    }
}
```

### 实例：GitStatus Provider

```rust
pub struct GitStatusProvider;

#[async_trait]
impl ContextProvider for GitStatusProvider {
    fn id(&self) -> &str { "git_status" }
    async fn provide(&self, req: &TurnRequest) -> anyhow::Result<Option<ContextFragment>> {
        if !req.env.is_git { return Ok(None); }
        let out = tokio::process::Command::new("git")
            .args(["status","--porcelain=v1","-b"])
            .current_dir(&req.env.cwd).output().await?;
        if !out.status.success() { return Ok(None); }
        Ok(Some(ContextFragment {
            tag: "git_status".into(),
            body: String::from_utf8_lossy(&out.stdout).into_owned(),
            cacheable: false,          // 每轮都可能变
            priority: 50,
        }))
    }
}
```

## 10.5 防止"上下文污染"

Agent 跑几十轮后上下文会**变得不可读**：

- 重复的 tool 结果
- 失败重试留下的干扰
- 过时的文件内容（已修改但旧读取还在 history 里）

### 对策清单

1. **摘要压缩**：第 8 章已讲
2. **过期标记**：在 tool_result 中注入 "file version hash"，下次引用同文件时前面的 result 可被标记 stale
3. **结构化 TODO**：让主 Agent 维护一份 TODO 列表，放在 system 末尾，而不是让模型在长对话里找
4. **Subagent 隔离**：让子 Agent 在独立上下文里做脏活（第 14 章）

## 10.6 "Lost in the middle" 的实战对策

研究表明模型对**开头和结尾**最敏感。Agent 实战：

- **最重要的指令放 system 开头 + 结尾各一遍**（重复不罗嗦，效果极好）
- 用户真正的问题放在最后一条 user message
- 工具调用结果放中间，不怕被稍微"淡化"

## 10.7 IDE / 环境上下文

Claude Code 会注入 `<ide_selection>`、`<open_files>`、`<terminal_output>`。对标的 Rust 做法：

- 定义一个 IPC 协议（stdin JSON 或 LSP-like），让 IDE 扩展把当前状态推给 Agent runtime
- Runtime 作为 `ContextProvider` 在每轮注入

## 10.8 小结

- **同一模型**，上下文工程能拉开一个数量级的效果
- Context Builder 把 system / memory / IDE / tool 都做成可插拔的 Provider
- 注意缓存边界、注意污染、关键信息首尾各一遍

> **下一章**：在给 Agent 更多能力前，先给它装"刹车"——权限与沙箱。

