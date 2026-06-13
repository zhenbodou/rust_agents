# 第 9 章 什么是 Harness Engineer

> 这是一个 2024 年后兴起的岗位，招聘市场上薪资甚至超过传统算法工程师。本章搞清楚：**它到底做什么，为什么值钱，以及你怎么成为一个。**

## 9.1 一个类比

想象 LLM 是赛车引擎——功率惊人，但你不能直接坐在引擎上开。你需要：

- **底盘**（runtime / agent loop）
- **方向盘**（prompts / tools）
- **刹车**（permissions / sandboxing）
- **仪表盘**（observability）
- **安全带**（evals / guardrails）
- **副驾**（subagents / validators）

**这套"引擎之外的一切"叫 harness，设计这套东西的人叫 Harness Engineer。**

另一个类比：模型是芯片，harness 是主板——芯片再强，没有一块好主板，性能发挥不出来。大厂之所以愿意为 Harness 工程师开出高薪，是因为：**同一个 GPT-4 / Claude 3.5 底下，harness 做得好的团队产品效果远超同行**，而 harness 搭建能力是非常稀缺的。

## 9.2 职责地图

```text
┌─────────────────────────────────────────────────────┐
│                 Harness Engineer                    │
├─────────────────────────────────────────────────────┤
│ 1. Agent Loop / Runtime 设计                        │
│ 2. 工具体系 (读写、shell、浏览器、DB…)               │
│ 3. Context Engineering (system / memory / docs)     │
│ 4. 权限与沙箱                                       │
│ 5. Hooks 与可扩展点                                 │
│ 6. Skills / Slash Commands / Workflows              │
│ 7. Subagent 编排与并行                              │
│ 8. Prompt Caching 与成本控制                        │
│ 9. Evals 与回归测试                                 │
│ 10. 可观测性 (traces, metrics, logs)                │
│ 11. 安全 (prompt injection, data exfil)             │
│ 12. 打包、分发、升级                                │
└─────────────────────────────────────────────────────┘
```

你可以看出，**这 12 条里只有 3 条和 LLM 直接相关**（工具体系、Context Engineering、Prompt Caching 涉及 LLM）。其余 9 条都是**系统工程**。这就是为什么资深后端/基础设施工程师转型 Harness 有巨大优势——你 95% 的技能可以直接迁移。

## 9.3 Harness Engineer vs 其他 AI 岗位

| 岗位 | 关注点 | 核心技能 | 典型招聘方 |
|---|---|---|---|
| ML 研究员 | 模型训练、论文 | PyTorch, CUDA, 数学 | 研究院、大模型公司 |
| Applied ML / MLOps | 模型微调、RAG、部署 | 向量库、Python、MLflow | 各类 AI 应用公司 |
| **Harness Engineer** | **LLM 外的全部系统** | **TS/Rust/Go, 系统设计** | **Claude Code、Cursor 类产品团队** |
| Prompt Engineer | 单次效果提升 | 提示技巧、测试 | 内部工具团队 |
| Agent Product Engineer | 用户体验、产品 | React, 业务逻辑 | 面向 C 端的 AI 产品 |
| AI Safety Engineer | 对齐、安全 | 数学、哲学、测试 | Anthropic、OpenAI、DeepMind |

**Harness Engineer 最接近的上游岗位**：高级后端工程师、基础设施工程师、Developer Tools 工程师。**转型成本最低，上手最快，薪资溢价最高。**

## 9.4 市场行情（2025/2026 数据）

根据公开 offer 记录和职场平台数据（仅供参考）：

| 地区 | 级别 | 年薪范围（USD 等价）|
|---|---|---|
| 美国湾区 | L4/Senior | $280K–$450K total comp |
| 美国湾区 | L5/Staff | $380K–$600K+ |
| 国内一线 | P6/高级 | ¥80–140 万 |
| 国内一线 | P7/资深 | ¥120–220 万 |
| 欧洲 | Senior | €120K–€200K |

**为什么这么高？**

供需不平衡：需要的人懂系统工程又懂 LLM 行为，这个交集的人极少。传统系统工程师不懂 LLM，传统 ML 工程师不懂分布式系统和可靠性工程。本书的目标就是帮你填满这个交集。

## 9.5 典型 JD 逐条拆解

真实的 Harness Engineer JD（综合自 Anthropic、Block/Goose、Sierra、Cognition 等公司）：

```
职位：AI Agent Infrastructure Engineer
要求：
1. 设计和优化 Agent 的 rollout 基础设施，支持大规模 RL 训练
2. 构建和维护 Agent 沙箱环境：隔离性、可复现性、高吞吐量
3. 设计 Agent 的可观测系统：轨迹、指标、告警、回放
4. 打通 Agent 框架（LangChain/LangGraph/OpenAI Agents SDK）与内部训练平台
5. 参与设计 tool use 协议、权限系统和 MCP 集成

加分项：
- 有 Rust/Go 后端经验
- 熟悉 K8s、容器安全（gVisor/Firecracker）
- 了解 RL 训练流程（PPO/GRPO）
```

每一条和本书的对应关系：

| JD 条目 | 本书对应章节 |
|---|---|
| rollout 基础设施 | ch43 Agent × RL |
| 沙箱环境 | ch11 权限、ch47 Agent 容器服务 |
| 可观测系统 | ch15 可观测性、ch38 轨迹查看器、Part 10 |
| 框架接入 | ch42 框架对接 |
| 工具协议、权限、MCP | ch11 权限、ch12 Hooks、ch14a MCP |
| Rust/Go | Part 5 mini-claude-code |
| K8s、容器安全 | ch45–ch47 |
| RL 训练 | ch43 |

**这不是巧合**，本书的内容架构就是根据这类 JD 反向设计的。

## 9.6 典型的 Harness 工作日常

举例一个真实任务：

> "Claude Code 发现在 Rust 大仓库里跑 `cargo check` 经常超 2 分钟，用户抱怨 Agent 卡死。请解决。"

一个 Harness Engineer 会做：

1. **诊断**：日志里看出 tool 串行执行、timeout 默认 30s、失败后模型重试 3 次
2. **设计**：
   - 为 `run_bash` 增加 **streaming output**，模型在长命令跑的时候看到进度
   - 提供 **background job** 机制：长命令进后台，用 `check_job_status(id)` 轮询
   - **智能 timeout**：对 `cargo check` 类命令默认 5 分钟，可配置
3. **实现**：改 tool schema、agent loop、session state
4. **Eval**：跑一套"长命令 + 超时 + 并发" evals 确保不回归
5. **观测**：添加 `tool_execution_duration` metric

再举一个偏 RL infra 的任务：

> "模型训练团队反映 rollout 时沙箱冷启动要 5 秒，导致 GPU 利用率只有 40%，请提升到 85%+。"

一个 Harness Engineer 会做：

1. **数据分析**：拆解 5 秒花在哪——镜像拉取 2s、容器启动 1.5s、依赖安装 1.5s
2. **沙箱池化**：第 47 章的池化方案，预热一批容器待命，取出直接用
3. **镜像优化**：镜像分三层（base / runtime / task），base 层几乎不变，运行时缓存命中率 95%
4. **异步 rollout**：不等慢任务，用上一版本模型的轨迹做误差修正
5. **效果验证**：GPU 利用率从 40% 提升到 88%，写 postmortem 和新的 SLO

这就是 Harness 日常。**它不是训练模型，而是让模型能在大规模生产环境里可靠、高效地运行**。

## 9.7 转型路径

根据你的背景，转型建议：

| 你的背景 | 已有的优势 | 需要补充 | 预计转型时间 |
|---|---|---|---|
| 后端工程师（3年+）| 分布式、数据库、API | LLM API 用法、Agent 概念、Eval 方法论 | 2–4 个月 |
| 基础设施/SRE | K8s、容器、可观测性 | LLM API、工具设计、Context Engineering | 2–3 个月 |
| 前端工程师 | TypeScript/React | 后端、Rust/Go、系统设计基础 | 4–6 个月 |
| 算法/ML 工程师 | 模型原理、Python | 系统工程、Rust/Go、生产可靠性 | 3–5 个月 |
| 应届/初级 | 学习能力强、无包袱 | 全栈系统工程基础 | 6–12 个月 |

**转型的最快路径**：做出来一个能展示的作品（本书的 mini-claude-code + agent-eval-platform），比任何证书都有说服力。

## 9.8 本部分接下来讲什么

- **第 10 章 Context Engineering**：上下文即产品——同一个模型，谁的 context 好谁的产品好
- **第 11 章 权限与沙箱**：别让 Agent 删你的 `.git`——最小权限原则的工程实现
- **第 12 章 Hooks**：事件驱动的插件机制——让第三方能在不改核心代码的情况下扩展 Agent
- **第 13 章 Skills**：可打包的能力单元——把能力封装成可复用、可分发的模块
- **第 14 章 Subagents**：编排多个 Agent 并行——突破单 Agent 的边界
- **第 14a 章 MCP 协议**：跨框架的工具标准——让 Agent 生态互联互通

每一章都会配套 Rust 代码。读完后你会有一整套可复用的 Harness 模块——这就是 Part 5 mini-claude-code 的地基。

## 9.9 小结

- Harness = LLM 之外的全部系统；12 条职责里只有 3 条和模型本身相关
- Harness Engineer ≈ 系统工程师 + LLM 领域知识；是转型 AI 工程**成本最低、溢价最高**的切口
- 2025/2026 市场薪资：国内 P6/P7 ¥80–220 万，湾区 L4/L5 $280K–$600K+
- 核心 JD 需求：rollout 基础设施、沙箱、可观测性、框架接入、MCP——这本书覆盖全了
- 最快转型路径：背景不重要，做出来两个能展示的项目最重要

> **下一章**：Context Engineering —— 同一个模型，上下文好坏决定产品高下。
