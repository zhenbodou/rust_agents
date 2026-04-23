# 附录 A · 常见问题 FAQ

## Q1: 没有 Rust 基础能看懂这本书吗？

能看懂概念，写代码可能吃力。建议先读《Rust 程序设计语言》(Rust Book) 前 10 章，再回来。本书着重 Agent 架构，Rust 只是载体。

## Q2: 没有 Anthropic API Key 怎么办？

- 使用国内 OpenAI 兼容服务（DeepSeek、Kimi、通义、豆包），改 `base_url`
- 本地 Ollama 起 Llama 3 / Qwen 3（8B 模型效果一般，32B 勉强能跑 Agent）
- Claude 在香港 / 海外区可直接注册

## Q3: mini-claude-code 可以商用吗？

MIT 许可。但生产用请做好：安全审计、LLM 服务费用控制、用户隐私合规（GDPR / 个保法）。

## Q4: 和 Cursor / Claude Code / Continue 的区别？

它们是工业级产品，我们的 mini-claude-code 是**教学级**。目标是让你**懂原理**能自己做一个，不是取代它们。

## Q5: 为什么不用 Python？

Rust 带来：静态二进制、启动快、内存安全、并发优势、部署简单。用 Python 一天可能就写完，但这本书的定位是"让你能进入基础设施 / harness 岗位"，Rust 更对口。

## Q6: 模型换了会不会所有章节都过时？

不会。本书 80% 是 Agent Runtime / Harness 工程，**与具体模型无关**。模型升级只会让 Agent 更好用，核心架构稳定。

## Q7: 我跑示例遇到 401 / 429 / 网络错误？

- 401：检查 `ANTHROPIC_API_KEY` 是否生效，`dotenvy::dotenv()` 在读取 env 之前
- 429：第 17 章 throttling + 指数退避
- 网络：国内访问 Anthropic 需要稳定的出口

## Q8: Subagent 会不会陷入递归？

我们在 `ToolContext` 里有 `depth` 字段，上限 3。超过直接拒绝。

## Q9: Prompt 里有时间戳会不会影响 cache？

会。所有变量放在 cache_control 标记**之后**的块里，稳定块保持每字节一致。

## Q10: 看完可以直接面试吗？

建议先把 mini-claude-code 跑通、推到 GitHub、写 README、录 demo。**有作品 + 本书内容消化 = 基本能应对主流面试。**

