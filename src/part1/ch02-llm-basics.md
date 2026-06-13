# 第 2 章 LLM 工作原理与 Agent 的关系

> 目标：让你**不学数学**也能说清 LLM 是怎么工作的，并理解为什么它能当 Agent 的"大脑"。

## 2.1 LLM 的一句话定义

**LLM（Large Language Model）是一个"输入一段文本，预测下一个 token 概率分布"的巨大函数。**

就这么一句话，别被论文吓到。

- **Token**：不是"字"也不是"词"，是 LLM 的最小单位。英文大约 1 token ≈ 4 字符；中文 1 token ≈ 0.5–1 个汉字。
- **预测**：模型对词表里每个 token 算一个概率，采样一个，拼回文本，再预测下一个……循环往复，就"写出"了答案。

> 你可以把 LLM 想象成一个**自动补全**，只不过它补全的是任意长度的复杂推理。

## 2.2 Transformer：LLM 的心脏（直觉版）

所有主流 LLM（GPT、Claude、Gemini、Llama）都基于 **Transformer** 架构（2017 年，"Attention Is All You Need"）。不需要会推导，但需要理解核心直觉：

### 注意力机制（Attention）

传统 AI 处理文本是逐字扫描、按顺序记忆，像人倒背如流。Transformer 的"注意力"更像**阅读理解**——对每个词，它都能直接"看"到序列里所有其他词，并根据相关性动态加权。

用"翻译"举例：翻译"**它**打败了对手，因为**它**非常强壮"中的第二个"它"时，模型需要判断这个"它"指的是"打败者"还是"对手"。注意力机制让模型能直接比较"它"和句子里的所有名词，找到最相关的那个。

**对 Agent 开发者的意义**：模型不是简单地"记住"上下文，而是对每个生成的 token 都重新计算整个上下文的相关性。这也解释了为什么：

- 把关键信息放在上下文的开头和结尾效果更好（Lost in the Middle 现象）
- 上下文变长时推理延迟线性增长（计算量与上下文长度是平方关系）
- 结构化的 context（清晰的分隔、标题）比混乱的 context 效果好

### 为什么叫"大"语言模型

规模（参数量）在 Transformer 这个架构里带来了**涌现能力**（emergent abilities）：在某个参数量阈值之后，模型突然能做之前完全不会的事情（如代码调试、逻辑推理），而不是平滑提升。GPT-3（175B）和 GPT-3.5（175B + RLHF）之间的能力差距，就来自对齐训练而非规模。

## 2.3 为什么"自动补全"能当 Agent

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

## 2.4 模型是怎么"训练"出来的

理解训练流程，有助于你判断模型的能力边界和失效场景。

### 阶段一：预训练（Pretraining）

在互联网规模的文本上（万亿 token：网页、书籍、代码、论文……）做自监督学习——给模型看"A B C"，让它预测下一个词"D"。这个阶段让模型积累了海量知识，但不会"对话"，输出往往是原文风格的续写。

这阶段的产物叫**基础模型**（Base Model），比如 Llama 3、Mistral base。

### 阶段二：监督微调（Supervised Fine-Tuning, SFT）

用人工标注的"(指令, 理想回答)"对训练模型。让模型学会"被问问题时应该回答问题而不是续写"、"被要求写代码时应该给出代码块而不是评论代码"。这让基础模型变成"能对话的模型"。

### 阶段三：人类反馈强化学习（RLHF / RLAIF）

让人类（或另一个 AI 模型，即 RLAIF）在多个回答里选出最好的。用这些偏好数据训练一个"奖励模型"，再用 PPO/GRPO 等强化学习算法让生成模型向高奖励方向优化。

这个阶段产生了 ChatGPT 式的"有帮助、无害、诚实"特性，也是工具调用能力显著提升的阶段——因为能调工具的回答被人类标注为更有帮助。

```
基础模型（海量知识但不对话）
     │ SFT
     ▼
对话模型（能对话但不一定有帮助）
     │ RLHF / RLAIF
     ▼
对齐模型（有帮助、无害、会工具）←── 这是你用 API 拿到的模型
```

**对 Agent 开发者的意义**：你调的 API（claude-sonnet-4-6、gpt-4o）是走完了全部三个阶段的最终产品。理解这个流程能帮你：

- 理解为什么模型有"知识截止日期"（预训练数据的时间限制）
- 理解为什么模型会拒绝某些请求（RLHF 的安全边界）
- 理解为什么换一个词措辞有时候能得到更好的结果（提示词影响模型的"解码路径"）

## 2.5 关键参数，只讲你需要的

| 参数 | 作用 | 实战建议 |
|---|---|---|
| `temperature` | 随机性，0–1 | Agent 场景 **0.0–0.3**，要稳定 |
| `max_tokens` | 单次生成上限 | Agent 通常 4096–16384 |
| `top_p` | 采样截断 | 一般默认 1.0，不用动 |
| `stop_sequences` | 遇到就停 | 高级用法，本书第 10 章用到 |
| `system` | 系统提示词 | Agent 的"人格设定"，极其关键 |

### temperature 的深层含义

Temperature 不是简单的"创意度"旋钮，它影响的是**从概率分布采样的方式**：

- `temperature=0`：每次都选概率最高的 token（贪婪解码），输出完全确定
- `temperature=1`：按照原始概率分布采样，有随机性
- `temperature>1`：拉平分布，低概率词也有更高机会被选到，输出更"意外"

Agent 为什么要低温度？因为 Agent 要执行的是**命令式任务**，你要的是可预期的工具调用，而不是创意写作。用高温度的 Agent 在不同运行之间行为差异大，难以 debug。

## 2.6 上下文窗口（Context Window）

LLM 能"看"的文字量是有限的。Claude Opus 4/Sonnet 4 支持 1M token，GPT-4o 128K，国产模型多在 32K–200K。

**上下文即一切**：模型不知道你仓库长什么样，不知道你上次聊了什么——除非你把这些信息放进上下文里。这是 **Part 3** 讲 Context Engineering 的核心动机。

### 有感觉的数字

- 1M tokens ≈ 75 万中文字 ≈ 一本《三体》三部曲
- 128K tokens ≈ 一本《哈利·波特》
- 但**塞得越满，模型越容易"迷失在中间"**（Lost in the Middle 效应）

### Lost in the Middle 效应

研究发现（2023 Liu et al.），即使模型支持很长的上下文，它对上下文**开头**和**结尾**的信息利用率远高于**中间**。实践建议：

- 关键指令放在 system prompt 开头
- 最新的工具结果放在 messages 末尾
- 长文档不要整体塞入，用 RAG 截取最相关的段落

这就是 **Context Engineering** 的起点——不是"能放多少就放多少"，而是"把对的信息放在对的位置"。

## 2.7 模型选型

截至 2026 年初，主流编程 Agent 可用的模型：

| 模型 | 厂商 | 强项 | 适用场景 |
|---|---|---|---|
| Claude Opus 4 / Fable 5 | Anthropic | 代码 / Agent Tool Use 顶级 | 生产主力 |
| Claude Sonnet 4.6 | Anthropic | 平衡速度与能力 | 日常调用 |
| Claude Haiku 4.5 | Anthropic | 快、便宜 | Subagent、辅助 |
| GPT-4o / o-series | OpenAI | 推理强 | 复杂规划 |
| Gemini 2.x Pro | Google | 超长上下文 | 大仓库分析 |
| DeepSeek V3 / Qwen | 国内 | 成本优势 | 国内部署 |

**给 Agent 选模型的黄金法则**：

1. 主 Agent 用最强模型（Opus/Fable 级）
2. Subagent 用中等模型（Sonnet）
3. 简单分类、摘要、路由用最便宜的（Haiku）
4. Embedding 用专门的 embedding 模型（text-embedding-3-small 等）

## 2.8 Tokenizer 与计费

你付钱是按 token 算的。记住两条：

- **输入 token** 便宜（一般是输出的 1/5–1/3）
- **输出 token** 贵

所以"让模型读很多，写很少"是成本优化的第一原则。第 16 章会详细讲 Prompt Caching——能再省 90%。

### Rust 里怎么数 token？

使用 [`tiktoken-rs`](https://crates.io/crates/tiktoken-rs)（OpenAI 分词器，可近似估算）：

```rust
use tiktoken_rs::cl100k_base;

fn main() -> anyhow::Result<()> {
    let bpe = cl100k_base()?;
    let tokens = bpe.encode_with_special_tokens("Hello, Agent world!");
    println!("tokens: {:?}, count: {}", tokens, tokens.len());
    Ok(())
}
```

Anthropic 有自己的 tokenizer，精确计数要走官方的 `/v1/messages/count_tokens` 接口（第 4 章示例）：

```rust
// 精确计数（Anthropic API），伪代码，第4章展开
let count = client
    .post("https://api.anthropic.com/v1/messages/count_tokens")
    .json(&count_req)
    .send().await?
    .json::<CountTokensResponse>().await?;

println!("input tokens: {}", count.input_tokens);
```

## 2.9 幻觉（Hallucination）与 Agent 的关系

LLM 会**一本正经地胡说**。这在纯聊天场景顶多丢脸，在 Agent 里**会删错文件、提错 commit**。

幻觉产生的原因有两类：

1. **知识截止**：训练数据有时间截止，模型不知道截止日期后发生的事，但可能"猜"一个看起来合理的答案
2. **过度自信**：模型倾向于给出一个答案而不是说"我不知道"，因为这样的回答在 RLHF 中被标注者评为更"有帮助"

Agent 能部分缓解幻觉，**但也会放大错误**：一旦错误的工具调用结果进入上下文，后续决策会被污染（即第 1 章的"幻觉传播"失败模式）。

**对抗幻觉的三板斧**（后面章节会逐一展开）：

1. **Grounding**：让模型引用真实材料（文件、DB 结果）而不是凭空编
2. **Verification**：关键操作之前用另一个"检查者"Agent 校验
3. **Permissions**：再怎么幻觉也动不了生产库（沙箱 / 白名单）

## 2.10 模型的"思考"：Chain of Thought 与推理模型

现代 LLM 在给出最终答案前，可以被引导先输出推理过程——这叫 **Chain of Thought（CoT）**。

```
普通问答：
用户：189 × 43 = ?
模型：8127（有时会算错）

Chain of Thought：
用户：请逐步计算 189 × 43
模型：189 × 40 = 7560，189 × 3 = 567，7560 + 567 = 8127 ✓
```

CoT 的价值在 Agent 场景里特别大：

- 模型会在调工具前先"想一想"要不要调、调哪个
- 中间推理步骤可以被你 log 下来用于 debug
- 错误的推理链可以被另一个 Agent 检查

OpenAI 的 o-series（o1、o3）和 Claude 的 Extended Thinking 是把"思考"做到了模型内部，原理类似。

**实践建议**：复杂规划任务（"设计一个架构方案"、"写一份迁移计划"）用带 thinking 的模型；工具调用密集的任务（"读文件改代码"）用普通模型，快且便宜。

## 2.11 小结

- LLM 是 token 预测器；Transformer 的注意力机制让它能"全局"看上下文
- 训练三阶段：预训练（积累知识）→ SFT（学会对话）→ RLHF（有帮助、会工具）
- 工具调用是"模型输出特殊格式文本，宿主程序解析执行"——这是 Agent 的底层机制
- Agent 的性能上限 = **模型能力** × **上下文质量** × **工具设计**
- 实战要点：温度调低（0–0.3）、注意 Lost in the Middle、关键信息放开头/结尾、成本靠 token 节省和 Prompt Caching

> **下一章**：搭建 Rust 开发环境，并建立贯穿全书的 Cargo workspace。
