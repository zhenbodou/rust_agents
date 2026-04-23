//! 第 13 章：Skills + Slash Commands 加载器（纯离线可运行）。

use anyhow::Result;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize, Clone)]
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

#[derive(Debug, Clone)]
pub struct Skill {
    pub manifest: SkillManifest,
    pub instructions: String,
    pub dir: PathBuf,
}

pub struct SkillLoader;

impl SkillLoader {
    pub async fn load_all(root: &Path) -> Result<Vec<Skill>> {
        let mut skills = Vec::new();
        if !root.exists() {
            return Ok(skills);
        }
        let mut rd = tokio::fs::read_dir(root).await?;
        while let Some(e) = rd.next_entry().await? {
            if !e.file_type().await?.is_dir() { continue; }
            let dir = e.path();
            let manifest_path = dir.join("skill.md");
            if !manifest_path.exists() { continue; }

            let raw = tokio::fs::read_to_string(&manifest_path).await?;
            let (fm, body) = split_frontmatter(&raw)?;
            let manifest: SkillManifest = serde_yaml::from_str(fm)?;
            let extra = tokio::fs::read_to_string(dir.join("instructions.md"))
                .await
                .unwrap_or_default();

            skills.push(Skill {
                manifest,
                instructions: format!("{body}\n\n{extra}"),
                dir,
            });
        }
        Ok(skills)
    }
}

fn split_frontmatter(raw: &str) -> Result<(&str, &str)> {
    let stripped = raw
        .strip_prefix("---")
        .ok_or_else(|| anyhow::anyhow!("no frontmatter"))?;
    let end = stripped
        .find("\n---")
        .ok_or_else(|| anyhow::anyhow!("frontmatter unterminated"))?;
    Ok((&stripped[..end], &stripped[end + 4..]))
}

#[derive(Debug, Clone)]
pub struct SlashCommand {
    pub name: String,
    pub body: String,
    pub description: String,
}

pub struct SlashRegistry {
    commands: HashMap<String, SlashCommand>,
}

impl SlashRegistry {
    pub async fn load(dir: &Path) -> Result<Self> {
        let mut commands = HashMap::new();
        if !dir.exists() {
            return Ok(Self { commands });
        }
        let mut rd = tokio::fs::read_dir(dir).await?;
        while let Some(e) = rd.next_entry().await? {
            if e.path().extension().and_then(|s| s.to_str()) != Some("md") {
                continue;
            }
            let raw = tokio::fs::read_to_string(e.path()).await?;
            let (fm, body) = split_frontmatter(&raw)?;
            #[derive(Deserialize)]
            struct Fm {
                command: String,
                description: String,
            }
            let f: Fm = serde_yaml::from_str(fm)?;
            commands.insert(
                f.command.clone(),
                SlashCommand {
                    name: f.command,
                    body: body.trim().into(),
                    description: f.description,
                },
            );
        }
        Ok(Self { commands })
    }

    pub fn resolve(&self, user_input: &str) -> Option<String> {
        let token = user_input.trim().split_whitespace().next()?;
        let cmd = self.commands.get(token)?;
        let rest = user_input.trim_start_matches(token).trim();
        if rest.is_empty() {
            Some(cmd.body.clone())
        } else {
            Some(format!("{}\n\n用户补充参数：{}", cmd.body, rest))
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let tmp = tempfile::tempdir()?;

    // 构造一个示例 Skill
    let skill_dir = tmp.path().join("skills/pr-reviewer");
    tokio::fs::create_dir_all(&skill_dir).await?;
    tokio::fs::write(
        skill_dir.join("skill.md"),
        "---\nname: pr-reviewer\ndescription: PR 评审员\ntriggers: [\"/review\"]\nmodel: claude-opus-4-7\nrequires_tools: [\"read_file\",\"run_bash\"]\n---\n\n当触发时：git diff，然后逐文件审阅。\n",
    ).await?;
    tokio::fs::write(skill_dir.join("instructions.md"), "审查时关注：安全、性能、可读性、测试。\n").await?;

    let skills = SkillLoader::load_all(&tmp.path().join("skills")).await?;
    for s in &skills {
        println!(
            "- {} ({}): triggers={:?}",
            s.manifest.name, s.manifest.description, s.manifest.triggers
        );
    }

    // 构造一个 slash command
    let cmd_dir = tmp.path().join("commands");
    tokio::fs::create_dir_all(&cmd_dir).await?;
    tokio::fs::write(
        cmd_dir.join("test.md"),
        "---\ncommand: /test\ndescription: run cargo nextest\n---\n执行 `cargo nextest run` 并总结失败\n",
    ).await?;

    let slash = SlashRegistry::load(&cmd_dir).await?;
    if let Some(expanded) = slash.resolve("/test --all") {
        println!("\n`/test --all` =>\n{expanded}");
    }
    Ok(())
}
