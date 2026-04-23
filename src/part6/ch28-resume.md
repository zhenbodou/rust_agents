# 第 28 章 简历、项目与作品集

> 学到这里，你已经比 80% 应聘 "AI Agent 工程师" 的人都更系统。现在要把知识变成 offer。

## 28.1 目标岗位画像

截至 2026 年，以下岗位都在招这套技能：

| 岗位 Title | 典型公司 | 侧重 |
|---|---|---|
| AI Agent Engineer | OpenAI 生态创业 / 国内 LLM 大厂 | 全栈 Agent 开发 |
| Harness Engineer | Anthropic / Cursor / Replit | Agent runtime + 工具 |
| Developer Experience (DX) for AI | 字节 / 腾讯 AI Lab | 开发者工具 |
| Applied AI Engineer | 各行业 AI 应用公司 | 业务集成 |
| AI Platform Engineer | 大厂 AI 平台组 | 多租户 infra |
| LLM SRE / Evals Engineer | 金融 / 医疗 AI | 可靠性 + 评估 |

薪资区间（一线城市，3–5 年经验）：P7 级 40–80w/年；P8 70–130w/年；美企远程 $160k–$300k。

## 28.2 简历骨架

硬技术岗简历 = **一个强项目 + 3 个佐证细节 + 清晰技术栈**。

### 一份示范简历（节选）

> **张三**  · AI Agent 工程师
> zhang@example.com · github.com/zhangsan · 微信 / Telegram: ...
>
> ### 主要项目：mini-claude-code（2026.02–至今）
> 
> 用 Rust 从零实现的类 Claude Code 编码助手，公开于 GitHub ⭐ xxx。
>
> - **Agent Runtime**：基于 Tool-calling loop 设计，支持流式 SSE、并发 tool 执行、可取消与预算控制。单次请求 P50 首 token 延迟 800ms，支持最多 40 轮 tool 循环。
> - **Harness**：实现细粒度权限系统（allow/deny/ask，glob + regex），Pre/Post Tool Hook（与 Claude Code JSON 协议兼容），Subagent fan-out（3 倍成本优化）。
> - **可观测性**：基于 tracing + OpenTelemetry 的全链路 trace，每轮 input/output/cache token 指标上 Prometheus。Prompt cache 命中率 0.74。
> - **安全**：实现 6 层纵深防御（system → tag 隔离 → 权限 deny → 沙箱 → 凭据扫描 → eval 红队），已用 50 个 prompt-injection 样本回归测试。
> - **测试**：自建 eval 框架（YAML 驱动），集成 CI 门禁，主干 pass-rate ≥ 92%。
>
> 技术栈：Rust (tokio, reqwest, ratatui), Anthropic / OpenAI 兼容 API, OpenTelemetry, ripgrep lib, bwrap。
>
> ### 深度技术文章
> - 《Prompt Caching 降本 90% 的实战》博客阅读 8k+
> - 《从零设计 Agent 的权限系统》GitHub Gist ⭐ 500+

**要点分析**：

- 项目讲清楚"做什么 / 怎么做 / 结果怎样"——数据说话
- 技术细节显露深度（"P50 延迟"、"cache 命中率"）
- 有公开的 GitHub 仓库 + 博客 = 可验证
- 把 **Harness Engineering 关键词**（权限、Hook、Subagent、eval、caching）全部展示出来

## 28.3 GitHub 作品集该长什么样

**一个精品项目 > 五个半成品**。推荐把 mini-claude-code 打磨到：

- README 第一屏有 demo GIF + 一行核心价值
- "Quick start" 5 分钟能跑起来
- 有 docs/ 详细架构文档
- CI 徽章 + 覆盖率徽章 + 版本徽章
- CHANGELOG.md 规范
- CONTRIBUTING.md + issue templates
- 至少 10 条有意义的 commit（而不是一次 push 完）

### Readme 模板

```markdown
# mini-claude-code

> A local-first coding agent in Rust, inspired by Claude Code.

![demo](./docs/demo.gif)

## Why another coding agent?

- 🦀 **100% Rust** — static binary, fast startup, zero-GC
- 🔒 **Security first** — glob/regex permissions + sandbox + secret scanning
- 🧩 **Hook-compatible with Claude Code settings.json**
- 🧠 **Subagent fan-out** cuts long-horizon cost by 3×

## Install

  cargo install mini-claude-code
  mcc   # runs the TUI

## Architecture

See [docs/architecture.md](docs/architecture.md).

## License
MIT
```

## 28.4 博客 / 公众号 / YouTube

3 篇技术博客能让你从简历海里脱颖而出：

1. **"用 Rust 写一个 Claude Code：架构篇"**
2. **"Prompt Caching：工程师绝不能忽视的降本武器"**
3. **"Agent 里的 Prompt Injection 防御实战"**

每篇 2000–4000 字，配代码 + 架构图。发在掘金 / 知乎 / dev.to / Medium 同步。面试官搜你名字能找到 = 大加分。

## 28.5 开源贡献

选一个活跃项目：

- `rig` (Rust 的 LLM framework)
- `swiftide`
- `oxc` / `biome` 与 AI 集成部分
- 国内：`eino`（字节 Agent 框架）

提 1–2 个有深度的 PR（不要只改错别字）。在简历里写明 "Contributed X to Y project"。

## 28.6 Demo 视频

30 秒演示：TUI 启动 → 用户输入 → Agent 跑起来 → 工具并行 → 产出结果。

ASCII-Cast / 录屏 → 放 README。面试官看 30 秒比读 30 分钟文字有用得多。

## 28.7 LinkedIn / 脉脉

标题建议：
> AI Agent Engineer | Built mini-claude-code in Rust | Harness engineering, observability, evals

正文前 3 行讲清楚你做什么，链接 GitHub + 博客。

## 28.8 小结

- 作品集 >>> 证书
- 一个深挖的项目，三篇技术博客，GitHub 整洁活跃
- 关键词：Harness、权限、Hook、Subagent、Caching、Eval、Observability

> **下一章**：40 道高频面试题，把你学到的东西训练成肌肉记忆。

