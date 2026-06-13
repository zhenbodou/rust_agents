# 附录 B · 术语表

> 按首字母/拼音排序。中英文双向索引，方便查阅。

## A–C

| 术语 | 英文 | 解释 |
|---|---|---|
| 智能体 | Agent | LLM + 工具 + 循环决策的组合系统。与 Chatbot 的区别：控制流由 LLM 自主决定 |
| Agent Loop | Agent Loop | Agent 的核心执行循环：接收用户输入 → 调 LLM → 决策 → 执行工具 → 回到 LLM，直到结束 |
| 对齐 | Alignment | 让模型行为符合人类意图和价值观的研究与工程领域 |
| 注意力机制 | Attention | Transformer 的核心：生成每个 token 时对上下文所有 token 动态加权 |
| 基础模型 | Base Model | 只经过预训练、尚未经过 SFT/RLHF 的原始模型，如 Llama base |
| 预算 | Budget | 对 token / 美元 / 迭代次数的上限，防止 Agent 失控 |
| 缓存击穿 | Cache Miss | Prompt Caching 前缀不匹配，未命中缓存，需重新计算 |
| 思维链 | Chain of Thought (CoT) | 提示模型"逐步思考"，输出推理过程后再给出答案，提升复杂任务准确率 |
| 检查点 | Checkpoint | Agent 执行过程中的状态存档点，用于崩溃恢复和人工介入 |
| 熔断 | Circuit Breaker | 连续失败后快速失败不再重试，保护下游服务 |
| 紧凑化 | Compact | 把旧消息摘要为简短摘要，释放上下文空间 |
| 宪法 AI | Constitutional AI | Anthropic 的对齐方法：让模型根据原则自我评估和修正输出（RLAIF 的一种形式）|
| 上下文工程 | Context Engineering | 动态构造每次 LLM 请求的 messages / system 的工程实践；上下文即产品 |
| 上下文窗口 | Context Window | 模型一次能处理的最大 token 数。Claude Sonnet/Opus：1M；GPT-4o：128K |

## D–F

| 术语 | 英文 | 解释 |
|---|---|---|
| 数据外泄 | Data Exfiltration | Agent 被 Prompt Injection 操控，把敏感数据发送到攻击者服务器 |
| 策略决定 | Decision (Allow/Deny/Ask) | 权限系统三态：允许 / 拒绝 / 询问用户 |
| 深度研究 | Deep Research | OpenAI/Perplexity 的产品形态：Agent 自主搜索、阅读、归纳，完成耗时研究任务 |
| 差异应用 | Diff Application | 将 diff/patch 格式应用到文件的操作，是代码编辑 Agent 的核心工具 |
| 直接偏好优化 | DPO | Direct Preference Optimization，RLHF 的替代算法，无需单独训练奖励模型 |
| 嵌入 | Embedding | 把文本映射到高维向量空间的表示，语义相近的文本向量距离近 |
| 涌现能力 | Emergent Abilities | 模型规模超过某阈值后突然具备的能力，在小模型上完全观察不到 |
| Episode | Episode | RL 训练中一次完整任务执行（从初始状态到终态）的过程 |
| 评估 | Eval | 对 Agent 行为的系统性测试与评分。分：结果评估、过程评估、LLM-as-Judge |
| 扩展思考 | Extended Thinking | Claude 在生成最终回答前在"沙盒"中进行的内部推理过程（不可见于 API 响应）|
| 幻觉 | Hallucination | 模型生成看似合理但事实错误的内容；在 Agent 里会引发操作失误 |
| 函数调用 | Function Calling | OpenAI 对 Tool Use 的叫法，等价。现在 OpenAI 也统称 Tool Use |
| 微调 | Fine-tuning | 在预训练模型基础上用专有数据进行有监督训练，使模型专业化 |
| 少样本学习 | Few-shot Prompting | 在提示词里给出少量示例（input→output 对），引导模型按此格式输出 |

## G–L

| 术语 | 英文 | 解释 |
|---|---|---|
| 基础化 | Grounding | 让模型输出基于真实可验证的材料（文件内容、数据库记录），减少幻觉 |
| 护栏 | Guardrail | 对 Agent 输入/输出的约束和过滤机制，防止有害或不合规的行为 |
| 脚手架（宿主层） | Harness | 包裹 LLM 的运行时系统：权限、工具、上下文、Hook、Subagent 等 |
| 人类反馈强化学习 | RLHF / RLHF | Reinforcement Learning from Human Feedback，用人类偏好数据对齐模型 |
| 移交 | Handoff | Agent 把任务控制权转移给另一个 Agent 的操作（OpenAI Agents SDK 的概念）|
| 钩子 | Hook | Agent 生命周期中的扩展点（工具前/后、消息前/后），允许注入自定义逻辑 |
| 上下文中间丢失 | Lost in the Middle | 模型对上下文开头和结尾的利用率远高于中间部分的现象 |
| 幻觉传播 | Hallucination Propagation | 一个错误的工具结果进入 context，后续决策全部建立在错误基础上 |
| 大语言模型 | LLM | Large Language Model，基于 Transformer、在海量文本上预训练的大规模语言模型 |
| 评审官 | LLM-as-Judge | 用另一个 LLM 对 Agent 的输出打分（比人工评分快，比规则灵活）|
| 日志概率 | Logprobs | 模型生成每个 token 时的对数概率；可用于置信度估计和 RL 训练 |

## M–P

| 术语 | 英文 | 解释 |
|---|---|---|
| 记忆 | Memory | 跨会话持久化的信息。分：短期（context）、长期（文件/DB）、语义（向量）|
| 模型上下文协议 | MCP | Model Context Protocol，Anthropic 推的 Agent 工具标准协议，跨框架互通 |
| 多 Agent 系统 | Multi-Agent System | 多个 Agent 协作完成任务的体系；分编排型（orchestrator + worker）和对等型 |
| 编排者 | Orchestrator | 多 Agent 系统中负责拆解任务、分配和调度子 Agent 的主 Agent |
| 并行工具调用 | Parallel Tool Use | 模型在一次响应中同时发起多个工具调用，由宿主并行执行，节省时间 |
| 权限系统 | Permission System | 控制 Agent 可以执行哪些操作的机制；通常分路径白名单、命令白名单等 |
| 策略 | Policy | RL 中从当前状态到下一步动作的映射函数；在 LLM 语境里就是模型本身 |
| 预训练 | Pretraining | 在互联网规模文本上用自监督学习训练基础模型的阶段 |
| 提示词缓存 | Prompt Caching | 服务端缓存 prompt 前缀的 KV 状态，命中后跳过重复计算，显著降低延迟和成本 |
| 提示词注入 | Prompt Injection | 攻击者通过不可信输入（工具结果、文件内容）向模型注入指令，劫持 Agent 行为 |

## R–S

| 术语 | 英文 | 解释 |
|---|---|---|
| 检索增强生成 | RAG | Retrieval-Augmented Generation，先用向量检索找相关文档片段，再喂给 LLM 生成 |
| ReAct | Reason + Act | 思考-行动-观察 交替的 Agent 范式：先推理再调工具，观察结果后再推理 |
| 奖励黑客 | Reward Hacking | RL 训练时模型钻奖励函数漏洞得高分，但没有真正完成任务 |
| 奖励模型 | Reward Model | RLHF 中训练的一个独立模型，用于给 Agent 的输出打分 |
| Rollout | Rollout | RL 训练中批量采集 Agent 执行轨迹的过程；Rollout 基础设施是 Harness 的核心职责 |
| 沙箱 | Sandbox | 物理隔离不可信代码执行的环境；Agent 沙箱要求：隔离强、供给快、可复现 |
| 会话 | Session | 一次连续对话的完整状态（消息历史 + 工具状态 + 元数据）|
| 技能 | Skill | 可打包的能力单元（说明文档 + 示例 + 工具依赖），可以被 Agent 加载使用 |
| 流式 | Streaming / SSE | 服务端用 Server-Sent Events 逐 token 推送，让用户看到实时输出 |
| 停止原因 | Stop Reason | API 响应结束的原因：end_turn（正常结束）/ tool_use（需要执行工具）/ max_tokens |
| 子智能体 | Subagent | 主 Agent 派生的、拥有独立上下文的 Agent，用于并行执行子任务 |
| 监督微调 | SFT | Supervised Fine-Tuning，用有标注的 (指令, 回答) 对微调基础模型 |
| 系统提示词 | System Prompt | 每次请求开头的特殊指令，设定 Agent 的角色、能力范围和行为规范 |

## T–Z

| 术语 | 英文 | 解释 |
|---|---|---|
| 温度 | Temperature | 控制 token 采样随机性的参数（0–1）；Agent 推荐 0–0.3，追求稳定可预期 |
| 节流 | Throttling / Rate Limit | 限制单位时间内的 API 请求数，防止过载或超出配额 |
| 令牌 | Token | LLM 的最小处理单位；英文 ≈ 4 字符/token，中文 ≈ 0.5–1 字/token |
| 工具模式 | Tool Schema | 描述工具的 JSON Schema（名称、参数、描述），告知模型该工具能干什么 |
| 工具使用 | Tool Use | 模型输出结构化调用请求、宿主程序执行后回写结果的机制 |
| 轨迹 | Trajectory | 一次 Agent 任务执行的完整记录（每一步的输入、输出、工具结果）|
| Transformer | Transformer | 2017 年提出的神经网络架构，基于注意力机制，是所有主流 LLM 的基础 |
| 向量数据库 | Vector DB | 专门存储和检索高维向量（Embedding）的数据库；常用：Qdrant、Weaviate、pgvector |
| 向量检索 | Vector Search | 在向量数据库中找出与查询向量最相近的结果（语义搜索）|
| 零样本学习 | Zero-shot Prompting | 不提供任何示例，直接让模型完成任务；现代大模型的强项 |
| 工作流 | Workflow | 开发者预先写死控制流的任务流水线；与 Agent 的区别：控制权在程序而非 LLM |
| 可观测性 | Observability | 通过日志（Logs）+ 链路追踪（Traces）+ 指标（Metrics）三支柱理解系统行为 |
