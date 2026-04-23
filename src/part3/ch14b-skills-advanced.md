# 第 14 章 补充 B · Skills 进阶：编写、分发、版本化

> 第 13 章讲了 Skill 的基础结构。本节带你走完从"能用"到"能发布到生态"的全流程。

## B.1 Skill 的完整形态

一个**发布级**的 Skill 目录结构：

```
pr-reviewer/
├── skill.md              # 必需：元数据 + 入口指令
├── instructions.md       # 可选：详细指令
├── examples/             # 可选：few-shot 示例
│   ├── 01-api-pr.md
│   └── 02-infra-pr.md
├── scripts/              # 可选：辅助脚本
│   └── collect-diff.sh
├── mcp/                  # 可选：伴生 MCP server
│   └── server.ts
├── CHANGELOG.md
├── LICENSE
└── README.md
```

## B.2 `skill.md` 完整字段

```markdown
---
name: pr-reviewer
version: 1.3.0                      # 遵循 semver
description: |
  一句话摘要 + 何时使用。
author: you@example.com
homepage: https://github.com/you/skills
license: MIT
# 激活条件
triggers:
  - "/review"
  - "review this PR"
# 运行时要求
requires:
  host_version: ">=0.1.0"           # 最低 mcc 版本
  tools: [read_file, run_bash, grep]
  mcp_servers: [github]             # 依赖的 MCP server
# 模型偏好
model:
  preferred: claude-opus-4-7
  fallback: claude-sonnet-4-6
# 成本守护
budget:
  max_usd: 0.50
  max_iterations: 20
# 产出 schema（可选但推荐）
outputs:
  format: json
  schema_ref: ./schemas/review.json
---

# PR Reviewer

## 何时使用
用户提到 "review"、"PR"、"pull request" 或显式输入 /review 时。

## 如何使用
1. 获取 diff：`git diff origin/main...`
2. 识别变更文件类型…
...
```

## B.3 编写高质量 Skill 的 8 条原则

1. **单一焦点**：一个 skill 做一件事做到底。"review" 和 "fix" 是不同 skill
2. **输入假设明确**：假设什么前置条件（比如 "必须在 git 仓库下跑"）
3. **Few-shot 真实**：examples 用真实重构过的案例，而不是玩具
4. **失败处理**：说明出错时怎么向用户解释
5. **不要无谓地调用工具**：skill 指令里要指引"什么情况下停"
6. **成本透明**：`budget` 字段让用户知道上限
7. **可测试**：配套 `tests/` 目录，几个 eval case
8. **版本化**：每次修改 bump version + CHANGELOG

## B.4 Rust 加载器升级

在 `examples/13-skills` 的基础上，新增 version / requires / budget 字段：

```rust
#[derive(Debug, Deserialize, Clone)]
pub struct SkillManifest {
    pub name: String,
    pub version: Option<String>,
    pub description: String,
    pub author: Option<String>,
    pub license: Option<String>,
    #[serde(default)]
    pub triggers: Vec<String>,
    #[serde(default)]
    pub requires: SkillRequires,
    #[serde(default)]
    pub model: SkillModel,
    #[serde(default)]
    pub budget: SkillBudget,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct SkillRequires {
    pub host_version: Option<String>,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub mcp_servers: Vec<String>,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct SkillModel {
    pub preferred: Option<String>,
    pub fallback: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct SkillBudget { pub max_usd: f64, pub max_iterations: u32 }
impl Default for SkillBudget {
    fn default() -> Self { Self { max_usd: 1.0, max_iterations: 25 } }
}
```

### 加载时校验兼容性

```rust
pub fn check_compatible(skill: &SkillManifest, host_version: &str, registered_tools: &[String], registered_mcp: &[String]) -> Result<(), String> {
    if let Some(req) = &skill.requires.host_version {
        let host_sem = semver::Version::parse(host_version).map_err(|e| e.to_string())?;
        let req_range = semver::VersionReq::parse(req).map_err(|e| e.to_string())?;
        if !req_range.matches(&host_sem) {
            return Err(format!("host {host_version} does not match {req}"));
        }
    }
    for t in &skill.requires.tools {
        if !registered_tools.iter().any(|x| x == t) {
            return Err(format!("required tool {t} not registered"));
        }
    }
    for s in &skill.requires.mcp_servers {
        if !registered_mcp.iter().any(|x| x == s) {
            return Err(format!("required MCP server {s} not configured"));
        }
    }
    Ok(())
}
```

## B.5 发布与分发：三条路

### B.5.1 Git repo（最简）

用户在 `~/.mcc/settings.json` 声明：

```json
{ "skillSources": [
  {"type": "git", "url": "https://github.com/you/skills", "path": "pr-reviewer", "rev": "v1.3.0"}
]}
```

Rust 加载：clone / pull → 拷贝到 `~/.mcc/skills/` 缓存。

```rust
pub async fn fetch_git_skill(src: &GitSource, cache_dir: &Path) -> Result<PathBuf> {
    let slug = slugify(&src.url);
    let repo_dir = cache_dir.join(slug);
    if !repo_dir.exists() {
        run(&["git", "clone", &src.url, repo_dir.to_str().unwrap()]).await?;
    } else {
        run_in(repo_dir.as_path(), &["git", "fetch"]).await?;
    }
    run_in(repo_dir.as_path(), &["git", "checkout", &src.rev]).await?;
    Ok(repo_dir.join(&src.path))
}
```

### B.5.2 Skill Registry（中心化分发）

模仿 crates.io / npm：

- 一个 HTTP 服务托管所有 skill tarball
- 每个 skill 有 `name@version`
- `mcc skills install pr-reviewer@1.3.0` 下载 + 校验 signature + 放到缓存

Server 端核心 API：

```
GET /v1/skills/{name}/{version}         -> tarball
GET /v1/skills/{name}/versions          -> [version]
GET /v1/skills/search?q=review          -> [meta]
```

### B.5.3 Inline Skill（最快迭代）

团队内部直接把 skill 目录 commit 到项目的 `.mcc/skills/`。项目 skill 永远覆盖全局同名 skill。

## B.6 签名与验证（安全关键）

Skill 跑 shell 命令、影响 LLM 行为、能把用户 prompt 送到外部 API——**完全等价于运行任意代码**。生产部署必须有签名。

推荐方案：[sigstore](https://www.sigstore.dev) 风格的 cosign + keyless 签名：

```bash
cosign sign-blob --bundle skill.sig pr-reviewer-1.3.0.tar.gz
# 发布时带上 skill.sig

# 安装端验证
cosign verify-blob --bundle skill.sig --certificate-identity author@company.com pr-reviewer-1.3.0.tar.gz
```

Rust 侧：

```rust
pub async fn verify_skill_tarball(tar_path: &Path, sig_path: &Path, expected_identity: &str) -> Result<()> {
    let status = Command::new("cosign").args([
        "verify-blob","--bundle", sig_path.to_str().unwrap(),
        "--certificate-identity", expected_identity,
        tar_path.to_str().unwrap(),
    ]).status().await?;
    if !status.success() { anyhow::bail!("signature verification failed"); }
    Ok(())
}
```

## B.7 Skill 的 evals

每个 skill 应该有配套 eval：

```
pr-reviewer/
└── tests/
    ├── cases/
    │   ├── 01-missing-tests.yaml
    │   └── 02-unsafe-unwrap.yaml
    └── run.sh          # 用 mcc eval run --suite tests/cases
```

CI 里：PR 改 skill → 自动跑 evals → 通过才合并。

## B.8 Slash Command / Skill 双绑定

一个 skill 可以声明多个 slash 入口：

```yaml
triggers:
  - "/review"
  - "/pr"
  - "/security-review"   # 激活同一 skill 但附加 --security-only 参数
```

Rust 里 Slash 解析器把 trigger 匹配到 skill + 把命令的剩余文本作为参数注入 skill 的指令开头。

## B.9 一个完整的企业级 Skill 案例

"Deploy Helper" Skill，做的事：

1. 听到 `/deploy`
2. 读 `.mcc/skills/deploy-helper/runbook.md`（按环境给步骤）
3. 依次：跑测试 → 构建镜像 → 调用 argocd MCP server → 等健康检查 → 发 slack 通知
4. 出错立刻回滚

关键配置片段：

```yaml
name: deploy-helper
version: 2.1.0
requires:
  tools: [run_bash, read_file]
  mcp_servers: [argocd, slack]
budget:
  max_usd: 1.5
  max_iterations: 30
triggers: ["/deploy"]
```

这是一个完整的"让团队所有人都能一致地发版"工具，也是 Harness Engineer 的日常。

## B.10 Skill 生态观察

截至 2026 年初：

- Anthropic 官方正在推 Skill 标准
- Claude Code、Cursor 都在支持类似机制
- MCP + Skill 组合是**未来 Agent 分发的两大基础设施**

**对求职者的启示**：在简历里放"我写过 X 个 Skill 包，包含 MCP server 和 cosign 签名"，是**极强的差异化信号**。

## B.11 小结

- 完整 Skill = manifest + instructions + examples + mcp（可选）+ tests
- Semver + CHANGELOG + requires 字段保证可升级
- 分发三路：Git / Registry / Inline
- 生产必须 cosign 签名
- Skill evals 进 CI 门禁

> **Part 3 真正完结**。这两小节让你在"MCP + Skills"这两个最热的生态点位站稳脚跟。

