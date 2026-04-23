# 第 2 章 LLM 工作原理与 Agent 的关系

> 目标：让你**不学数学**也能说清 LLM 是怎么工作的，并理解为什么它能当 Agent 的"大脑"。

## 2.1 LLM 的一句话定义

**LLM（Large Language Model）是一个"输入一段文本，预测下一个 token 概率分布"的巨大函数。**

就这么一句话，别被论文吓到。

- **Token**：不是"字"也不是"词"，是 LLM 的最小单位。英文大约 1 token ≈ 4 字符；中文 1 token ≈ 0.5–1 个汉字。
- **预测**：模型对词表里每个 token 算一个概率，采样一个，拼回文本，再预测下一个……循环往复，就"写出"了答案。

> 你可以把 LLM 想象成一个**自动补全**，只不过它补全的是任意长度的复杂推理。

## 2.2 为什么"自动补全"能当 Agent

这是很多人困惑的地方：**一个补全器怎么会调用工具？**

答案：**工具调用也是用文本表达的**。

当我们给模型看这样的训练样本（几十亿条）：

```text
[用户]：今天北京天气怎么样？
[助手]：<tool_use name="get_weather" input='{"city":"北京"}'/>
[工具结果]：25°C 晴
[助手]：北京今天 25 度，晴天。
```

模型学到了一种模式：**遇到需要外部信息时，输出一段"工具调用"的文本**。我们的代码只要识别这段特殊文本，实际执行工具，再把结果塞回去，模型就会继续"补全"。

**所以 Agent = 约定一种文本协议 + 在客户端执行这段文本描述的动作。**

## 2.3 关键参数，只讲你需要的

| 参数 | 作用 | 实战建议 |
|---|---|---|
| `temperature` | 随机性，0–1 | Agent 场景 **0.0–0.3**，要稳定 |
| `max_tokens` | 单次生成上限 | Agent 通常 4096–8192 |
| `top_p` | 采样截断 | 一般默认 1.0，不用动 |
| `stop_sequences` | 遇到就停 | 高级用法，本书第 10 章用到 |
| `system` | 系统提示词 | Agent 的"人格设定"，极其关键 |

## 2.4 上下文窗口（Context Window）

LLM 能"看"的文字量是有限的。Claude Opus 4.7 目前支持 1M token，GPT-4o 128K，国产模型多在 32K–200K。

**上下文即一切**：模型不知道你仓库长什么样，不知道你上次聊了什么——除非你把这些信息放进上下文里。这是 **Part 3** 讲 Context Engineering 的核心动机。

### 一个有感觉的数字

- 1M tokens ≈ 75 万中文字 ≈ 一本《三体》三部曲
- 但**塞得越满，模型越容易"迷失在中间"** (lost in the middle)

## 2.5 模型选型

截至 2026 年初，主流编程 Agent 可用的模型：

| 模型 | 厂商 | 强项 | 适用场景 |
|---|---|---|---|
| Claude Opus 4.7 | Anthropic | 代码 / Agent Tool Use 顶级 | 生产主力 |
| Claude Sonnet 4.6 | Anthropic | 平衡速度与能力 | 日常调用 |
| Claude Haiku 4.5 | Anthropic | 快、便宜 | Subagent、辅助 |
| GPT-4o / o-series | OpenAI | 推理强 | 复杂规划 |
| Gemini 2.x Pro | Google | 超长上下文 | 大仓库分析 |
| DeepSeek V3 / Qwen | 国内 | 成本优势 | 国内部署 |

**给 Agent 选模型的黄金法则**：

1. 主 Agent 用最强模型（Opus 级）
2. Subagent 用中等模型（Sonnet / Haiku）
3. Embedding / 简单分类用最便宜的

## 2.6 Tokenizer 与计费

你付钱是按 token 算的。记住两条：

- **输入 token** 便宜（一般是输出的 1/5–1/3）
- **输出 token** 贵

所以"让模型读很多，写很少"是成本优化的第一原则。第 16 章会详细讲 Prompt Caching——能再省 90%。

### Rust 里怎么数 token？

使用 [`tiktoken-rs`](https://crates.io/crates/tiktoken-rs)：

```rust
use tiktoken_rs::cl100k_base;

fn main() -> anyhow::Result<()> {
    let bpe = cl100k_base()?;
    let tokens = bpe.encode_with_special_tokens("Hello, Agent world!");
    println!("tokens: {:?}, count: {}", tokens, tokens.len());
    Ok(())
}
```

Anthropic 有自己的 tokenizer，精确计数要走官方的 `/v1/messages/count_tokens` 接口（第 4 章示例）。

## 2.7 幻觉（Hallucination）与 Agent 的关系

LLM 会**一本正经地胡说**。这在纯聊天场景顶多丢脸，在 Agent 里**会删错文件、提错 commit**。

Agent 能部分缓解幻觉，**但也会放大错误**：一旦错误的工具调用结果进入上下文，后续决策会被污染。

**对抗幻觉的三板斧**（后面章节会逐一展开）：

1. **Grounding**：让模型引用真实材料（文件、DB 结果）而不是凭空编
2. **Verification**：关键操作之前用另一个"检查者"Agent 校验
3. **Permissions**：再怎么幻觉也动不了生产库（沙箱 / 白名单）

## 2.8 小结

- LLM 是 token 预测器；工具调用是它**输出一段特殊文本**，由宿主程序解析执行
- Agent 的性能上限 = **模型能力** × **上下文质量** × **工具设计**
- 三大要点：温度调低、token 要省、幻觉要防

> **下一章**：搭建 Rust 开发环境，并建立贯穿全书的 Cargo workspace。

