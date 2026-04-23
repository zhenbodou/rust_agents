# 第 29 章 高频面试题 40 讲

> 按话题分类。每题都给关键答题提纲，重点技术细节请回翻对应章节。

## A. 基础概念（8 题）

**1. Agent 和 Workflow 的本质区别？**
Agent 的控制流由 LLM 动态决定；Workflow 由开发者写死。LLM 决策 → Tool 调用 → 观察 → 再决策的循环是 Agent 核心；Workflow 是硬编码 DAG。

**2. LLM 为什么能调用工具？**
把工具调用表示为一段约定的文本（`tool_use` 内容块）。训练数据里有大量 "遇到此类问题就输出工具调用" 的模式。宿主代码识别后执行并把结果写回上下文。

**3. Agent Loop 里哪些情况会导致死循环？怎么防？**
模型反复调用同一工具；工具一直失败；max_tokens 导致回复被截断。防护：`max_iterations` 上限、`budget_usd` 上限、工具调用去重检测、`max_tokens` 触发时明确终止或摘要。

**4. stop_reason 的几种值？**
`end_turn` / `stop_sequence` / `tool_use` / `max_tokens`。Loop 必须对每种明确处理。

**5. 温度参数对 Agent 意味着什么？**
低温（0.0–0.3）稳定、可重复，适合 tool call；高温多样性强但 tool 参数易错。Agent 默认 0.0。

**6. Context window 越大越好吗？**
不是。Lost in the middle 效应；越大越贵越慢；越大越容易污染。正确做法是 Context Engineering 精心装配。

**7. 幻觉对 Agent 的危害比 Chatbot 大吗？**
大。幻觉会触发错误的 tool 调用，错误结果污染后续上下文。对抗：grounding（读真实文件）、verifier 子 Agent、权限硬控。

**8. Agent 为什么要有 observability？**
长时间运行 + 多步 + 多工具 + 外部依赖，不可观测就不可运维。必须：日志、trace、metrics、session replay。

## B. Harness Engineering（10 题）

**9. 介绍下你理解的 Harness Engineer 职责。**
见第 9 章 12 条。重点强调：Agent Runtime、工具、Context、权限、Hook、Subagent、Caching、Eval、可观测、安全——系统工程而非 ML。

**10. 权限系统三模式的设计思路？**
default：读自动允许、写/执行询问；acceptEdits：写也允许；bypass：全自动。deny > allow > mode default > ask。deny 永远最高优先级，不能被覆盖。

**11. 怎么防 Prompt Injection？**
6 层：system prompt 警示 → 隔离标签 → 权限 deny → 沙箱 → 凭据扫描 → eval 红队。关键思想：不要依赖模型自觉，**宿主层硬控**。

**12. Subagent 的价值与陷阱？**
价值：上下文隔离、并行、便宜模型分流。陷阱：递归深度、预算失控、结论不可靠。对策：depth 限制、budget 限制、verifier。

**13. Hook 系统怎么设计？**
事件 + 匹配器 + 命令（shell 或内建）。8 种事件（Pre/PostToolUse、UserPromptSubmit、SessionStart/Stop、Notification、PreCompact、SubagentStop）。Pre 事件可阻止，Post 可改输出。

**14. 怎么做上下文的缓存命中？**
把稳定部分放前（system、工具 schema、skill instructions），cache_control 标记到末尾。监控 `cache_read_input_tokens` / 总 input 的比率，目标 >= 0.7。

**15. 如何控制一次 session 的成本？**
分层模型（主 Opus、sub Haiku）、预算硬上限、截断、Batch API、Prompt caching、压缩/摘要。

**16. Skill 与 Slash Command 的关系？**
Slash 是一行别名，触发词；Skill 是能力包（带 instructions、示例、可选工具）。Slash 常被用来触发 Skill。

**17. Tool 输出要截断吗？**
要。否则大文件 / 大 log 会炸上下文。头尾保留 + 中间省略 + 告知模型 "截断了 N 字节"。

**18. 什么时候该上 RAG？**
> 100 个文档，或文档量超过上下文窗口，或有高度非结构化资料。优先把长期记忆做成 Markdown；RAG 是最后的手段。

## C. 代码与系统设计（10 题）

**19. 设计一个 trait 让 Agent 支持多家 LLM Provider。**
`trait LlmProvider { fn complete; fn stream; }` + `CompleteRequest` / `MessageResponse` 归一化；Anthropic / OpenAI / 自研 各自 impl。第 4 章详述。

**20. Agent 一次回合内多个 tool_use，并发还是串行执行？**
并发。用 `tokio::spawn` fan-out，所有 tool 跑完再一起写回 user 角色 tool_result。但要注意：部分工具不是并发安全（如两个 edit_file 同一文件），可在 Tool trait 加 `is_concurrent_safe()` 标记串行组。

**21. 流式响应怎么一边更新 UI 一边拼完整 block？**
SSE 事件两路消费：向 UI 送 `TextDelta`，同时聚合到 `current_text`。`ToolUseStart` 时 flush text block；`ToolUseInputDelta` 累计 partial_json；`MessageStop` 时把最后一个 tool_use 也落成 block。

**22. 如何做 tool 调用的超时？**
三层：HTTP `timeout`、单 tool `tokio::time::timeout`、整 turn 外层 wrap。内层 < 外层。bash 子进程要 `kill_on_drop(true)` 或显式 SIGTERM→SIGKILL。

**23. 怎么实现 edit_file 的精确替换？**
"old_string 必须唯一命中"，否则失败并提示加上下文或 `replace_all`。写 tmp → rename 原子替换。

**24. Session 怎么存储才能 replay？**
JSONL 每轮一行：完整 `messages`（至该轮之前）+ assistant blocks + tool outputs + usage。加 `meta.json` 元数据（title、cost、status）。

**25. 如何防止 Rust Agent 里的内存无限增长？**
流式结果及时写回磁盘；messages 超阈值触发压缩；大文件只读 offset/limit；tool 输出 `MAX_OUTPUT_BYTES` 截断。

**26. 设计一个 circuit breaker。**
3 状态（closed / open / half-open）。连续失败 N 次打开；打开期间快速失败；冷却时间后进入 half-open 允许探测请求。见第 17 章代码。

**27. 一个 tool 失败，主 Agent 应该怎么办？**
不要 panic。把错误写成 `tool_result` `is_error: true`，让模型看到。模型可能重试、换参数、或放弃。**用 LLM 处理 LLM 能处理的问题**。

**28. 如何设计一个 eval 框架？**
YAML 数据集 → Runner 在临时 sandbox 布置 fixtures → 跑 Agent → 多种 checker（bash / contains / judge）→ 预算约束 → 聚合报告 → CI 门禁。

## D. 高级主题（7 题）

**29. Prompt caching 怎么省钱的原理？**
Anthropic 服务端缓存 prefix 的 KV。命中时只收 10% input token 费用。要求前缀逐字节一致；稳定在前、多变在后。最多 4 个 cache_control 标记。

**30. 什么是 lost in the middle？怎么缓解？**
长上下文中间部分被模型忽略。缓解：重要指令在首尾重复；用户当前问题放最后；关键决策做成结构化 TODO 放系统末尾。

**31. Agent 对同一个 prompt 结果不稳定，怎么排查？**
温度？采样？上下文有时间戳等变量导致不命中缓存？tool 结果里有时序信息？每次加的 context provider 是否稳定？用 session replay 对比两次的 exact messages diff。

**32. 如何让 Agent 自己决定调用哪个 subagent？**
在 system prompt 里列一个 "Available Subagents"目录；每条带"何时用"。`spawn_subagent(preset, task)` 让模型选 preset。

**33. Agent 要支持中断，Rust 里怎么优雅实现？**
`tokio_util::sync::CancellationToken`，所有 await 点用 `tokio::select!` 监听 token。子进程 `kill_on_drop` 或先 SIGTERM。UI 按 Esc 触发 cancel。

**34. 如何做 Agent 的 A/B 测试？**
按 session 哈希分桶；不同桶用不同 system prompt / 模型。记录相同指标（完成率、成本、用户打分），统计显著差异后切流。

**35. 多租户部署时如何隔离？**
每个 tenant 独立 api key、独立 cost quota、独立 session 存储 key（加密）、独立沙箱（microVM）。日志 tenant_id 打标。

## E. 行为面试（5 题）

**36. 描述一次你 debug 复杂 Agent 问题的经历。**
结构：现象 → 已有观测（日志/trace）→ 假设 → 验证 → 根因 → 修复 → 回归。引用你 mini-claude-code 里的真实案例。

**37. 你怎么平衡模型能力与成本？**
分层策略（主/子模型）、缓存、截断、batch、prompt 精简、评估驱动迭代。

**38. 你最得意的 Agent 设计决策？**
例如"edit_file 强制唯一匹配"——根因：防模型误伤其他代码，效果：回归率显著下降。讲权衡：增加了失败率但可控，比"静默覆盖"好。

**39. 对未来 1 年 Agent 技术的看法？**
比如长期趋势：多模态、computer use、更强的 planning、eval infra 成为一级公民。别夸海口，体现有独立思考。

**40. 你为什么选 Rust 做 AI 工具？**
启动快、内存安全、长跑稳定、部署简单（静态二进制）。适合 CLI、沙箱、基础设施。承认限制：生态不如 Python，但 I/O + agent harness 场景够用。

## 小结

- A 类考底子、B 类考视野、C 类考代码、D 类考深度、E 类考价值观
- 每题都能引用到你的 mini-claude-code 作品——这是你最大的护城河
- 面试不要背答案，要把答案和项目里的**真实决策**挂钩

> **下一章**：持续学习路线图，拿到 offer 只是开始。

