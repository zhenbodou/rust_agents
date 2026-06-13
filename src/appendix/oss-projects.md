# 附录 D · 真实生产级开源项目学习路线

> 书里的项目是教学最优解，真实生产代码是另一种营养——它包含历史包袱、性能妥协、issue 里的真实事故。本附录精选与 JD 各条目直接对应的生产级项目，每个给出"读什么、怎么读、读完做什么"。
>
> 读源码方法论：先跑起来 → 找入口（main/核心 loop）→ 顺一条完整请求的调用链 → 再看测试（测试是最诚实的文档）→ 最后看 issue/PR 学演化史。用 Claude Code 辅助导读："给我讲讲这个仓库的 X 是怎么实现的"是合法外挂。

## D.1 Agent Harness 类（对应 Part 3/5）

**OpenHands（原 OpenDevin）** — github.com/All-Hands-AI/OpenHands

生产级编码 Agent 平台，Python 后端 + React 前端，与本书评测平台架构惊人相似。
读：`openhands/controller/`（agent loop 与状态机）、`openhands/runtime/`（沙箱抽象：Docker/K8s/远程多实现）、`frontend/`（聊天与轨迹 UI）。
做：对比它的 EventStream 设计与本书 TraceEvent；找一个 good-first-issue 提 PR——**给这个项目贡献过代码本身就是简历素材**。

**Claude Agent SDK (Python)** — github.com/anthropics/claude-agent-sdk-python

Claude Code 内核的官方开放形态。
读：query 循环、工具注入、hooks 与 permission 回调的接口设计。
做：用它复刻 mini-claude-code 的功能，写一篇两者 harness 设计对比。

**goose** — github.com/block/goose

Block 开源的 **Rust** Agent 框架——证明 Rust 写 Agent 是生产现实而非本书一厢情愿。
读：`crates/goose/src/agents/`（loop 与 extension 系统）、MCP 集成层。
做：对比它和 mcc 的工具 trait 设计差异，给自己的 mcc 借鉴一个改进。

## D.2 RL 训练基建类（对应 ch43，JD 第 1 条核心）

**verl** — github.com/volcengine/verl

字节开源的 RL 训练框架，Agentic RL 的事实标准之一。**这个 JD 的内部系统大概率长得像它**。
读：`verl/workers/rollout/`（rollout worker 怎么组织）、agent loop 扩展点、`verl/trainer/`（轨迹怎么喂给 GRPO/PPO）。不用读懂算法实现，重点是**数据流和接口**。
做：跑通它的 GSM8K 例子；然后实现 ch43 练习——把自己的 harness 接成它的环境。

**agent-lightning** — github.com/microsoft/agent-lightning

微软开源，定位正是 JD 第 1 条："把任意 Agent 框架接入 RL 训练"——与 ch42/43 的统一适配层思想互为印证。
读：它的 trace 采集方式（sidecar 拦截 LLM 调用）、训练接口抽象。
做：对比它与你在 ch42 设计的适配层，各自的取舍写成笔记。

**SWE-bench / SWE-agent** — github.com/SWE-bench/SWE-bench, github.com/SWE-agent/SWE-agent

编码 Agent 评测的标准基准 + 配套 harness。
读：SWE-bench 的任务容器化方式（每个 issue 一个 Docker 环境——ch47 L3 镜像的真实版本）、评分隔离设计。
做：在自己的评测平台上跑 SWE-bench-lite 的 10 个 case。

## D.3 可观测与评测平台类（对应 Part 10，JD 第 3 条）

**Langfuse** — github.com/langfuse/langfuse

LLM 可观测平台，自托管开源，TypeScript 全栈。**就是本书评测平台的商业化形态**。
读：trace/span/generation 数据模型、ClickHouse + Postgres 双存储选型（对比本书 S3 + PG）、前端 trace 树渲染。
做：自托管部署一套，把 mcc 的轨迹通过 OTLP 打进去；对比它的存储选型与 ch50 的差异及原因。

**OpenTelemetry GenAI 语义约定** — opentelemetry.io/docs/specs/semconv/gen-ai/

不是项目是规范，但生产团队的轨迹模型正在向它收敛。读完把 TraceEvent 映射到 GenAI semconv，这是"标准化意识"的加分项。

## D.4 沙箱与基础设施类（对应 ch47/45，JD 第 2 条）

**E2B** — github.com/e2b-dev/E2B 与 **infra** 仓库

商业级"给 AI 跑代码的沙箱云"，Firecracker 实现。
读：infra 仓库的 microVM 编排、模板（镜像）构建管线、沙箱生命周期 API。
做：画出它的沙箱供给时序图，对比 ch47 沙箱池设计。

**gVisor** — github.com/google/gvisor

不要求读实现（用户态内核太硬核），读 docs/ 的架构文档 + 在 K8s 里实际配 RuntimeClass 跑一个沙箱即可。

## D.5 协议与生态类（对应 ch14a/42，JD 加分项 3）

**MCP 官方 spec + servers** — github.com/modelcontextprotocol

读：spec 的 transport 层（stdio/streamable HTTP）、servers 仓库里 filesystem server 的权限处理。
做：把你 ch42 练习的 MCP server 按 spec 完整实现 resources + prompts 能力。

**OpenAI Agents SDK** — github.com/openai/openai-agents-python

读：`src/agents/run.py` 的主循环（与你的 loop 对照）、handoff 与 guardrail 的实现（各 ~200 行，精巧）。

## D.6 学习路线建议

```
第 1 个月   verl 跑通例子 + OpenHands 部署使用 + 读两者核心目录
第 2 个月   Langfuse 自托管 + 接入自己的轨迹；SWE-bench 跑 10 case
            同时：给 OpenHands/Langfuse 提第一个 PR（文档/小修复也算）
第 3 个月   agent-lightning 或 E2B infra 深读其一
            把所有对比笔记整理成 3-5 篇博客 → 作品集
```

衡量"读懂"的标准不是看完，而是：能画出架构图、能讲清三个关键设计决策的 trade-off、能指出一处你会改的地方并说明理由。面试官问"你读过什么生产代码"时，这三样就是答案的骨架。
