# 附录 B · 术语表

| 术语 | 英文 | 解释 |
|---|---|---|
| 智能体 | Agent | LLM + 工具 + 循环决策 的组合系统 |
| 脚手架（宿主层） | Harness | 包裹 LLM 的运行时：权限、工具、上下文、Hook 等 |
| 上下文工程 | Context Engineering | 动态构造每次请求的 messages / system 的工程实践 |
| 工具使用 | Tool Use | 模型输出结构化调用请求、宿主执行并回写结果 |
| 函数调用 | Function Calling | OpenAI 叫法，等价 Tool Use |
| 指令微调 | Instruction Tuning | 让模型学会遵循自然语言指令的训练阶段 |
| ReAct | Reason + Act | 思考-行动-观察 交替的 Agent 范式 |
| 令牌 | Token | LLM 的最小处理单位 |
| 上下文窗口 | Context Window | 模型一次能"看"的 token 数 |
| 流式 | Streaming | 服务端用 SSE 逐 token 推送 |
| 提示词缓存 | Prompt Caching | 服务端缓存前缀 KV，命中后显著降费 |
| 子智能体 | Subagent | 主 Agent 派生的、独立上下文的 Agent |
| 钩子 | Hook | 生命周期事件的扩展点 |
| 技能 | Skill | 可打包的能力单元（说明 + 示例 + 工具依赖） |
| 评估 | Eval | 对 Agent 行为的测试与评分 |
| 评审官 | LLM-as-Judge | 用 LLM 给另一 LLM 的输出打分 |
| 沙箱 | Sandbox | 物理隔离不可信命令执行的环境 |
| 提示词注入 | Prompt Injection | 通过不可信输入让模型偏离指令 |
| 嵌入 | Embedding | 把文本映射到向量空间的表示 |
| 检索增强生成 | RAG | Retrieval-Augmented Generation，先检索再生成 |
| 会话 | Session | 一次连续对话的完整状态 |
| 记忆 | Memory | 跨会话持久化的事实 |
| 工作流 | Workflow | 开发者写死控制流的流水线 |
| 模型上下文协议 | MCP | Anthropic 推的 Agent/Tool 标准协议 |
| 策略决定 | Decision | 权限系统的三态：Allow / Deny / Ask |
| 压缩 | Compact | 把旧消息摘要以节省 token |
| 预算 | Budget | 对 token / 美元 / 迭代次数的上限 |
| 熔断 | Circuit Breaker | 连续失败后快速失败，保护下游 |
| 节流 | Throttling / Rate Limit | 限制单位时间请求数 |
| 可观测性 | Observability | 日志 + Trace + Metrics 三支柱 |

