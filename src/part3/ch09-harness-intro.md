# 第 9 章 什么是 Harness Engineer

> 这是一个 2024 年后兴起的岗位，招聘市场上薪资甚至超过传统算法工程师。本章搞清楚：**它到底做什么，为什么值钱。**

## 9.1 一个类比

想象 LLM 是赛车引擎——功率惊人，但你不能直接坐在引擎上开。你需要：

- **底盘**（runtime / agent loop）
- **方向盘**（prompts / tools）
- **刹车**（permissions / sandboxing）
- **仪表盘**（observability）
- **安全带**（evals / guardrails）
- **副驾**（subagents / validators）

**这套"引擎之外的一切"叫 harness，设计这套东西的人叫 Harness Engineer。**

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

你可以看出，**这 12 条里只有 3 条和 LLM 直接相关**。其余 9 条都是**系统工程**。这就是为什么资深后端/基础设施工程师转型 Harness 有巨大优势。

## 9.3 Harness Engineer vs 其他 AI 岗位

| 岗位 | 关注点 | 用的工具 |
|---|---|---|
| ML 研究员 | 模型训练 | PyTorch, GPU, 论文 |
| Applied ML | 模型微调、RAG | 向量库、Python |
| **Harness Engineer** | **LLM 外的全部系统** | **TS/Rust/Go, 系统设计** |
| Prompt Engineer | 单次效果提升 | 各种 prompt 技巧 |
| Agent Product Engineer | 用户体验、产品 | React, 业务逻辑 |

## 9.4 典型的 Harness 项目实际工作

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

这就是 Harness 日常。**它不是训练模型，而是让模型可用**。

## 9.5 本部分接下来讲什么

- **第 10 章 Context Engineering**：上下文即一切
- **第 11 章 权限与沙箱**：别让 Agent 删你的 `.git`
- **第 12 章 Hooks**：事件驱动的插件机制
- **第 13 章 Skills**：可打包的能力单元
- **第 14 章 Subagents**：编排多个 Agent 并行

每一章都会配套 Rust 代码。读完后你会有一整套可复用的 Harness 模块——这就是 Part 5 mini-claude-code 的地基。

## 9.6 小结

- Harness = LLM 之外的全部系统
- Harness Engineer ≈ 系统工程师 + 一些 LLM 领域知识
- 市场稀缺，是转型 AI 工程的最佳切口

> **下一章**：Context Engineering —— 同一个模型，上下文好坏决定产品高下。

