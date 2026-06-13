# 附录 A · 常见问题 FAQ

## 入门与环境

### Q1: 没有 Rust 基础能看懂这本书吗？

能看懂概念，写代码可能吃力。建议先读《Rust 程序设计语言》(The Rust Book) 前 10 章，再回来。本书着重 Agent 架构，Rust 只是载体。第 3 章有完整的环境搭建和工程脚手架说明。

### Q2: 没有 Anthropic API Key 怎么办？

有几个替代方案：

- **国内 OpenAI 兼容服务**：DeepSeek、Kimi（月之暗面）、通义千问（阿里）、豆包（字节），只需改 `base_url` 和 `api_key`，本书示例代码均支持
- **本地模型**：Ollama 运行 Llama 3 / Qwen 3（8B 效果一般，推荐 32B+，但需要较强的 GPU/内存）
- **Claude 注册**：香港、日本、新加坡区域可直接注册

替代服务的代码适配（第 4 章有详细说明）：

```rust
let client = Client::builder()
    .base_url("https://api.deepseek.com/v1")  // 改这里
    .api_key(std::env::var("DEEPSEEK_API_KEY")?)
    .build()?;
```

### Q3: 电脑配置要求？

Rust 编译会占用一些资源，最低建议：

- CPU：4 核（编译用，运行只需 1 核）
- 内存：8 GB（推荐 16 GB，同时开多个服务时更流畅）
- 硬盘：10 GB 空闲（Rust 工具链 + 依赖）
- 系统：macOS 12+、Ubuntu 22.04+、Windows 11 + WSL2

## 代码与调试

### Q4: 跑示例遇到 401 / 429 / 网络错误怎么办？

**401 Unauthorized**：API Key 未生效。排查顺序：
1. 确认 `.env` 文件存在于项目根目录且包含 `ANTHROPIC_API_KEY=sk-ant-...`
2. 确认代码里 `dotenvy::dotenv().ok()` 在 `std::env::var("ANTHROPIC_API_KEY")` **之前**调用
3. 用 `printenv ANTHROPIC_API_KEY` 确认 shell 环境变量是否生效

**429 Too Many Requests**：超出速率限制。解决方案见第 17 章（指数退避 + 抖动）。临时绕过可在请求间加 `tokio::time::sleep(Duration::from_secs(1)).await`。

**网络连接问题**：国内访问 Anthropic API 需要稳定的海外出口。如无法解决，使用 Q2 中的国内替代服务。

### Q5: `cargo build` 很慢怎么办？

Rust 首次编译确实较慢。几个加速方法：

```bash
# 1. 换国内 crates.io 镜像（~/.cargo/config.toml）
[source.crates-io]
replace-with = "ustc"
[source.ustc]
registry = "sparse+https://mirrors.ustc.edu.cn/crates.io-index/"

# 2. 用 sccache 缓存编译结果
cargo install sccache
export RUSTC_WRAPPER=sccache

# 3. 用 mold 链接器（Linux 快 3-5 倍）
sudo apt install mold
# .cargo/config.toml 里加：[target.x86_64-unknown-linux-gnu] linker = "clang" rustflags = ["-C", "link-arg=-fuse-ld=mold"]
```

### Q6: 示例代码和书里的代码有出入怎么办？

书里的代码有时为了讲清楚做了简化，实际可运行的完整版在 `examples/` 目录。遇到差异：
1. 以 `examples/` 目录的代码为准
2. 查看对应章节的 `CHANGELOG.md`
3. 在仓库提 issue 或查看已有 issue

## Agent 行为与调试

### Q7: Agent 跑着跑着卡住了怎么办？

常见原因和排查方法：

| 现象 | 可能原因 | 排查方法 |
|---|---|---|
| 长时间没输出 | 工具执行超时 | 看日志，工具是否有超时设置 |
| 一直在调同一个工具 | Agent 死循环 | 检查 max_iterations 限制 |
| 流式输出突然中断 | 网络超时或 max_tokens 设太小 | 看 stop_reason 是否是 `max_tokens` |
| 每次结果不一样 | temperature 太高 | 降低 temperature 到 0.1 以下 |

### Q8: Subagent 会不会陷入递归？

本书的实现在 `ToolContext` 里有 `depth` 字段，默认上限 3。超过直接拒绝并返回错误信息。可在构建 `ToolContext` 时自定义：

```rust
let ctx = ToolContext::new(tools, PermissionSystem::new())
    .with_max_depth(5);  // 改上限
```

实际生产中还应配合 token 预算（`budget_tokens`）和时间限制。

### Q9: Prompt 里有时间戳会不会影响 Prompt Cache 命中？

会。`cache_control: {"type": "ephemeral"}` 标记把 prompt 分割成"稳定前缀"和"变动后缀"两段，只缓存稳定前缀。

**正确做法**：所有会变的内容（时间戳、用户输入、工具结果）放在 cache_control 标记**之后**的块里，稳定块保持每字节完全一致。详见第 16 章。

### Q10: 工具调用结果太长导致 context 撑满怎么办？

三种策略：

1. **截断**：每个工具返回值设上限（如 8000 chars），超出截断并加提示 `[已截断，用 offset 参数获取后续内容]`
2. **分页**：为大输出添加 `offset` / `limit` 参数，让 Agent 按需翻页
3. **摘要**：对长文本工具（如读大文件）先压缩再返回，必要时提供原始内容访问

本书 `Read` 工具和 `Bash` 工具的实现都包含了自动截断逻辑（第 22 章）。

### Q11: Agent 经常"发明"不存在的文件路径怎么办？

这是"过度自信"幻觉（第 1 章失败模式之一）。对策：

1. 在系统提示词里加：*"在读取任何文件前，必须先用 ls/glob 工具确认文件存在"*
2. 读文件工具在文件不存在时返回明确的错误信息，而不是空字符串
3. 使用"先列目录再读文件"的工具调用序列作为 few-shot 示例

## 项目与求职

### Q12: mini-claude-code 可以商用吗？

MIT 许可，可以商用。但生产用请注意：

- **安全审计**：第 19 章的 Prompt Injection 防御要完整实现
- **费用控制**：第 16 章的 Prompt Caching + 第 17 章的限流是必须的
- **隐私合规**：用户对话数据按 GDPR / 个保法要求处理，不要发给未经用户同意的第三方

### Q13: 和 Cursor / Claude Code / Continue 的区别？

它们是工业级产品，mini-claude-code 是**教学级**。目标是让你**懂原理**，不是取代它们。

差异对比：

| 维度 | mini-claude-code | Claude Code 等 |
|---|---|---|
| 目的 | 理解原理、求职作品集 | 生产使用 |
| 代码量 | ~3000 行（刻意精简）| 数万到数十万行 |
| 功能 | 核心功能完整 | 功能齐全、稳定、有 UI |
| 许可 | MIT | 各家不同 |

学完本书后，你能读懂这些产品的源代码，甚至给它们贡献 PR。

### Q14: 为什么不用 Python？

Rust 带来几个对 Harness Engineer 岗位有利的特性：

- **静态二进制**：不需要安装 Python 运行时，部署简单
- **内存安全**：无 GC 停顿，高并发场景下性能更稳定
- **并发优势**：Tokio 异步运行时天然适合 I/O 密集的 Agent 工具执行
- **岗位信号**：会 Rust 的 AI 工程师在市场上极为稀缺，这是加分项

**如果你有 Python 背景**：Part 8 覆盖了 Python 版的 Agent 实现，并对接 LangChain/OpenAI Agents SDK 和 RL 训练。两者不冲突。

### Q15: 模型换了（比如出了新的 Claude 5）会不会所有章节都过时？

不会。本书 80% 是 Agent Runtime / Harness 工程，**与具体模型版本无关**。

唯一可能过时的部分：第 2、4 章关于具体模型名称和参数的表格。但这些只是参考，核心的工具调用模式、Agent Loop 设计、权限系统等自 GPT-4 Function Calling 以来就没有根本性变化。

### Q16: 看完可以直接面试吗？

建议先做到以下几点再去投：

1. **跑通 mini-claude-code**：能完整展示至少 3 个有实际价值的使用场景
2. **推到 GitHub**：写清楚的 README（是什么 / 为什么 / 怎么跑）
3. **录一个 2–3 分钟的 demo 视频**：面试时最快传递"你真的做出来了"的信号
4. **能解释关键设计**：权限系统为什么三态、Context Engineering 做了什么优化、如何防 Prompt Injection

**有作品 + 消化本书内容 = 基本能应对主流面试。** 第 29 章有 40 道高频面试题拆解。

## 生产与运维

### Q17: 如何处理 API 的速率限制（Rate Limit）？

完整方案见第 17 章。要点：

```rust
// 指数退避 + 全局限流器组合
use governor::{Quota, RateLimiter};

let limiter = RateLimiter::direct(Quota::per_minute(nonzero!(100u32)));
limiter.until_ready().await;   // 请求前先获取令牌
// 429 时：sleep(2^retry_count * base_delay + jitter)
```

还要在请求头里读取 `retry-after` / `anthropic-ratelimit-*` 头，按服务端指示等待。

### Q18: 怎么控制 Agent 的成本？

四层防线（从便宜到贵）：

1. **Prompt Caching**：稳定的系统提示词和文件内容缓存后节省 90% 输入成本（第 16 章）
2. **模型分层**：主 Agent 用强模型，Subagent 用便宜模型（第 2 章选型原则）
3. **工具输出截断**：防止大工具结果撑满 context（Q10）
4. **预算限制**：`budget_tokens` 设每任务 token 上限，超出自动中止

### Q19: 如何在多用户 / 多租户场景下隔离 Agent？

关键是**每个用户的 Agent 必须完全隔离**：

- **沙箱隔离**：每个 Agent 运行在独立的容器里（ch47），文件系统、进程、网络全隔离
- **API Key 隔离**：每个租户用独立的 API Key 或 sub-key，便于计费和限流
- **Session 隔离**：Session 数据按 user_id 分区，数据库行级权限控制
- **工具白名单**：不同租户可开放不同的工具集

### Q20: Agent 出问题了怎么排查？

排查工具箱（从浅到深）：

1. **看 traces**：第 15 章的 OpenTelemetry span，定位哪一步慢/出错
2. **看 LLM 输入**：打印每次发给 LLM 的完整 messages（生产里 debug 级日志）
3. **重放轨迹**：第 38 章的 Trace Viewer，逐步回放 Agent 的每个动作
4. **隔离工具**：把怀疑有问题的工具单独测试，排除 LLM 因素
5. **降温度跑**：设 temperature=0，排查随机性引入的问题

### Q21: 如何给 Agent 做回归测试？

核心思路见第 18 章。要点：

- 维护一个"黄金测试集"：覆盖核心功能和历史 bug 修复
- 每次 PR 自动跑测试集，用 LLM-as-Judge 打分，分数回退阻断合并
- 测试用例格式：`{input: "用户指令", expected_behaviors: [...], forbidden_behaviors: [...]}`

比传统单元测试难的地方：Agent 输出有随机性。解法是用"行为断言"而非"字符串匹配"，或固定 temperature=0 后做字符串比对。
