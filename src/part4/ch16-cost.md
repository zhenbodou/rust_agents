# 第 16 章 Prompt Caching 与成本优化

> 一个用 Anthropic Claude 的 Agent，如果不用 prompt caching，**多花 5–10 倍的钱**。这是最容易落下的一章。

## 16.1 Prompt Caching 是什么

Anthropic 允许你把 messages 中稳定的前缀标记为"可缓存"，服务端把 KV Cache 保存 5 分钟（默认）。下次请求若前缀相同，**只收 10% 的 input token 费用**，且延迟也大幅降低。

```json
{
  "system": [
    {"type": "text", "text": "长长的系统 prompt..."},
    {"type": "text", "text": "大量工具说明...", "cache_control": {"type": "ephemeral"}}
  ],
  "messages": [...]
}
```

`cache_control` 标记的块**及其前面所有内容**一起被缓存。下次只要完全一致就命中。

## 16.2 适合缓存的内容

| 内容 | 是否值得缓存 |
|---|---|
| 基础 system prompt | ✅ 必须 |
| 工具 schema | ✅ 必须 |
| Skill / Workflow 指令 | ✅ |
| 项目 CLAUDE.md | ✅ 稳定时 |
| memory index | ✅ 稳定时 |
| 当前 git status | ❌ 每次都变 |
| 用户本次问题 | ❌ 放最后 |

**排序原则**：**稳定的在前，多变的在后**。

## 16.3 Rust 实现

修改我们的消息结构支持 cache_control：

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text {
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
    // ...其他变体
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheControl {
    #[serde(rename = "type")]
    pub kind: String,   // "ephemeral"
}
```

注意：**cache_control 最多 4 个标记**（Anthropic 限制）。合理规划：

- 标记 1：系统 prompt 末尾
- 标记 2：工具定义末尾
- 标记 3：长期上下文（memory/CLAUDE.md）末尾
- 标记 4：session 历史的某个稳定节点

### 16.3.1 帮助函数

```rust
pub fn mark_cacheable(text: String) -> ContentBlock {
    ContentBlock::Text {
        text,
        cache_control: Some(CacheControl { kind: "ephemeral".into() }),
    }
}
```

### 16.3.2 构造请求时使用

```rust
let system_blocks = vec![
    ContentBlock::Text { text: BASE_SYSTEM.into(), cache_control: None },
    ContentBlock::Text {
        text: format!("# Tools available\n{tool_schemas}"),
        cache_control: Some(CacheControl { kind: "ephemeral".into() }),
    },
];
```

在 API body 里 `system` 字段是 array 时要对应传 blocks。Anthropic 的最新 API 接收 string 或 content blocks 数组。

## 16.4 缓存命中的验证

响应里有：

```json
{"usage": {
  "input_tokens": 50,
  "cache_creation_input_tokens": 0,
  "cache_read_input_tokens": 8000,
  "output_tokens": 120
}}
```

**cache_read_input_tokens 是关键**。Rust 记录：

```rust
tracing::info!(
    cache_hit = usage.cache_read_input_tokens > 0,
    cache_read = usage.cache_read_input_tokens,
    cache_write = usage.cache_creation_input_tokens,
    "llm usage"
);
```

## 16.5 其他成本杀招

### 16.5.1 分层模型

```rust
pub fn choose_model(task: TaskKind) -> &'static str {
    match task {
        TaskKind::Planning | TaskKind::Critical => "claude-opus-4-7",
        TaskKind::Routine | TaskKind::ToolCalls => "claude-sonnet-4-6",
        TaskKind::Summarize | TaskKind::Classify => "claude-haiku-4-5-20251001",
    }
}
```

主 Agent Opus，子 Agent Haiku，摘要用 Haiku——三档分流可省 60%+。

### 16.5.2 Token 上限与早停

```rust
if ctx.cost.input_tokens > 500_000 {
    return Err("session budget exceeded".into());
}
```

### 16.5.3 结果截断

- Tool 输出 > 64KB 强制截断 + 摘要
- 大文件阅读用 offset/limit，不要全量读

### 16.5.4 Prefill 减少生成

让模型少写无用寒暄。Prefill `"{"` 强制 JSON 开头；system 明确"直接给结论不要复述问题"。

### 16.5.5 Batch API

对异步可容忍场景（夜间批量 eval、回放），用 Anthropic Batch API，价格**直接 5 折**。

```rust
// 伪接口
let batch = client.create_message_batch(vec![req1, req2, ...]).await?;
// 24h 内可查结果
```

## 16.6 监控你的缓存命中率

每个 session 算一个指标：

```rust
let hit_rate = cache_read as f64 / (cache_read + input_tokens) as f64;
histogram!("agent_cache_hit_rate").record(hit_rate);
```

生产目标：**>= 0.7**。如果低于 0.3，大概率你：

- 把变量放在了前缀里
- system prompt 每次都带了时间戳
- 工具 schema 每次构造的 JSON 键顺序不一样（serde_json 保证了，但别手拼）

## 16.7 小结

- Prompt caching 是**必修**，不是优化
- 稳定在前、多变在后
- 分层模型 + 截断 + Batch 三连降本
- 把 `cache_hit_rate` 当作核心 SLO 监控

> **下一章**：让 Agent 跑得稳——错误处理、重试、限流、熔断。

