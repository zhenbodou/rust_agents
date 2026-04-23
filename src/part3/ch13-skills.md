# 第 13 章 Skills、Slash Commands 与 Workflows

> Skill 把"一类任务的专家能力"打包成可分发的单元。它是 Agent 生态的"npm 包"。

## 13.1 三者关系

| 概念 | 定位 | 例子 |
|---|---|---|
| **Slash Command** | 用户在输入框敲的快捷方式 | `/test`, `/review` |
| **Workflow** | 一段有序步骤的指导手册 | "发版流程" |
| **Skill** | 可复用的能力包（含提示词 + 工具 + 文档） | "pr-reviewer", "security-audit" |

Slash command 往往**触发** Skill 或 Workflow。Skill 可以包含多个 Slash command。

## 13.2 Skill 的标准格式

参考 Anthropic 的约定，一个 Skill 是一个目录：

```
skills/
└── pr-reviewer/
    ├── skill.md          # 入口：何时用、怎么用、参数
    ├── instructions.md   # 详细指令（system prompt 追加）
    ├── examples/         # few-shot 示例
    └── scripts/          # 可选的辅助脚本
```

`skill.md` 前置元数据：

```markdown
---
name: pr-reviewer
description: |
  在用户要求 code review 或提到 PR 时使用。
  擅长指出安全、性能、可读性、测试覆盖问题。
triggers:
  - "/review"
  - "review this PR"
model: claude-opus-4-7
requires_tools: [read_file, run_bash, grep]
---

# PR Reviewer Skill

当触发时：
1. 先 `git diff origin/main...` 获取变更
2. 读相关文件理解上下文
3. 按 critical / major / minor 三档给意见
4. 最后建议下一步动作
```

## 13.3 Rust 实现

### 13.3.1 数据结构

```rust
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct SkillManifest {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub triggers: Vec<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub requires_tools: Vec<String>,
}

pub struct Skill {
    pub manifest: SkillManifest,
    pub instructions: String,   // 整段 Markdown
    pub dir: std::path::PathBuf,
}
```

### 13.3.2 加载器

```rust
pub struct SkillLoader;

impl SkillLoader {
    pub async fn load_all(root: &Path) -> anyhow::Result<Vec<Skill>> {
        let mut skills = Vec::new();
        let mut rd = tokio::fs::read_dir(root).await?;
        while let Some(e) = rd.next_entry().await? {
            if !e.file_type().await?.is_dir() { continue; }
            let dir = e.path();
            let manifest_path = dir.join("skill.md");
            if !manifest_path.exists() { continue; }
            let raw = tokio::fs::read_to_string(&manifest_path).await?;
            let (fm, body) = split_frontmatter(&raw)?;
            let manifest: SkillManifest = serde_yaml::from_str(fm)?;
            let extra = tokio::fs::read_to_string(dir.join("instructions.md")).await.unwrap_or_default();
            skills.push(Skill {
                manifest,
                instructions: format!("{body}\n\n{extra}"),
                dir,
            });
        }
        Ok(skills)
    }
}

fn split_frontmatter(raw: &str) -> anyhow::Result<(&str, &str)> {
    let stripped = raw.strip_prefix("---").ok_or_else(|| anyhow::anyhow!("no frontmatter"))?;
    let end = stripped.find("\n---").ok_or_else(|| anyhow::anyhow!("frontmatter unterminated"))?;
    Ok((&stripped[..end], &stripped[end+4..]))
}
```

### 13.3.3 触发器匹配

用户输入 "帮我 review 这个 PR" → 应该激活 `pr-reviewer`。方案：

1. **字面匹配**：`/review` 这种 slash 显式触发，高优先级
2. **语义匹配**：用 LLM 在前置一轮决定 "需要激活哪些 skill"

```rust
pub struct SkillRouter { skills: Vec<Arc<Skill>> }

impl SkillRouter {
    pub fn match_slash(&self, input: &str) -> Option<Arc<Skill>> {
        let cmd = input.trim().split_whitespace().next()?;
        self.skills.iter().find(|s| s.manifest.triggers.iter().any(|t| t == cmd)).cloned()
    }

    /// 所有 skill 的"目录"，放进 system prompt 让模型自己决定
    pub fn catalog_for_prompt(&self) -> String {
        let mut s = String::from("# Available Skills\n");
        for sk in &self.skills {
            s.push_str(&format!("- **{}** — {}\n", sk.manifest.name, sk.manifest.description.lines().next().unwrap_or("")));
        }
        s
    }
}
```

### 13.3.4 激活后怎么用

把 Skill 的 `instructions` **作为一段 user message 或 system 追加**注入本轮上下文：

```rust
pub fn apply_skill_to_request(req: &mut CompleteRequest, skill: &Skill) {
    let banner = format!(
        "\n\n# Activated Skill: {}\n{}\n",
        skill.manifest.name, skill.instructions
    );
    // 追加到 system 末尾 —— 最新指令位置最显眼
    let base = req.system.take().unwrap_or_default();
    req.system = Some(format!("{base}{banner}"));

    if let Some(model) = &skill.manifest.model {
        req.model = model.clone();
    }
}
```

## 13.4 Slash Command 作为最简 Skill

很多时候你只想敲 `/test` 跑测试。这是不需要完整 Skill 目录的"一行指令"：

```markdown
---
command: /test
description: Run cargo nextest and summarize failures
---

执行 `cargo nextest run`，如有失败列出每个失败的测试名和错误摘要。
```

`SlashRegistry`:

```rust
pub struct SlashCommand { pub name: String, pub body: String, pub description: String }

pub struct SlashRegistry { commands: HashMap<String, SlashCommand> }

impl SlashRegistry {
    pub async fn load(dir: &Path) -> anyhow::Result<Self> {
        let mut commands = HashMap::new();
        let mut rd = tokio::fs::read_dir(dir).await?;
        while let Some(e) = rd.next_entry().await? {
            if e.path().extension().and_then(|s| s.to_str()) != Some("md") { continue; }
            let raw = tokio::fs::read_to_string(e.path()).await?;
            let (fm, body) = split_frontmatter(&raw)?;
            #[derive(Deserialize)]
            struct Fm { command: String, description: String }
            let f: Fm = serde_yaml::from_str(fm)?;
            commands.insert(f.command.clone(), SlashCommand { name: f.command, body: body.into(), description: f.description });
        }
        Ok(Self { commands })
    }

    pub fn resolve(&self, user_input: &str) -> Option<String> {
        let token = user_input.trim().split_whitespace().next()?;
        let cmd = self.commands.get(token)?;
        // 简单：用 body 替换触发词
        let rest = user_input.trim_start_matches(token).trim();
        Some(format!("{}\n\n用户补充参数：{}", cmd.body, rest))
    }
}
```

这样用户敲 `/test` 实际发给模型的是上面 markdown 的 body。

## 13.5 工业实践：Skill 目录的组织

```
.agent/
├── settings.json                 # 权限 + hooks
├── skills/
│   ├── pr-reviewer/
│   ├── security-audit/
│   ├── rust-refactor/
│   └── release-notes/
├── commands/                     # 一行命令
│   ├── test.md
│   ├── build.md
│   └── deploy.md
└── hooks/                        # 钩子脚本
```

团队可以把 `.agent/` 放到 git，全员共享。这正是 Claude Code 社区生态的雏形。

## 13.6 Skills + Context Caching = 省钱神技

大多数 Skill instructions 是**稳定**的。把它们放在 messages 前缀，配合 prompt caching（第 16 章），90% 情况下命中缓存，token 费用降 10 倍。

## 13.7 小结

- Slash Command：一行别名
- Skill：完整能力包（提示词 + 工具要求 + 示例）
- Workflow：多步脚手架（本质也是 Skill 的一种）
- Rust 实现是"加载 → 匹配 → 追加到 system prompt"

> **下一章**：Subagents —— 让主 Agent 把脏活累活分给小弟。

