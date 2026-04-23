//! 第 8 章：Markdown 长期记忆存储（核心可离线跑的部分）。

use anyhow::Result;
use std::path::PathBuf;
use tokio::fs;

#[derive(Debug, Clone)]
pub struct MemoryEntry {
    pub name: String,
    pub kind: String,
    pub tags: Vec<String>,
    pub description: String,
    pub body: String,
}

pub struct MarkdownMemoryStore {
    pub root: PathBuf,
}

impl MarkdownMemoryStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub async fn save(&self, entry: &MemoryEntry) -> Result<()> {
        fs::create_dir_all(&self.root).await?;
        let file = self.root.join(format!("{}.md", sanitize(&entry.name)));
        let body = format!(
            "---\nname: {}\ntype: {}\ntags: [{}]\ndescription: {}\nupdated: {}\n---\n\n{}\n",
            entry.name,
            entry.kind,
            entry.tags.join(", "),
            entry.description,
            chrono::Utc::now().to_rfc3339(),
            entry.body,
        );
        fs::write(file, body).await?;
        self.update_index().await?;
        Ok(())
    }

    pub async fn load_all(&self) -> Result<Vec<MemoryEntry>> {
        let mut out = Vec::new();
        if !self.root.exists() {
            return Ok(out);
        }
        let mut rd = fs::read_dir(&self.root).await?;
        while let Some(e) = rd.next_entry().await? {
            let path = e.path();
            if path.extension().and_then(|s| s.to_str()) != Some("md") {
                continue;
            }
            if path.file_name().map(|n| n == "MEMORY.md").unwrap_or(false) {
                continue;
            }
            let raw = fs::read_to_string(&path).await?;
            if let Some(entry) = parse_entry(&raw) {
                out.push(entry);
            }
        }
        Ok(out)
    }

    pub async fn index_for_prompt(&self) -> Result<String> {
        let path = self.root.join("MEMORY.md");
        if !path.exists() {
            return Ok(String::new());
        }
        Ok(fs::read_to_string(path).await?)
    }

    async fn update_index(&self) -> Result<()> {
        let entries = self.load_all().await?;
        let mut idx = String::from("# Memory Index\n\n");
        for e in entries {
            idx.push_str(&format!(
                "- [{}]({}.md) — {}\n",
                e.name,
                sanitize(&e.name),
                e.description
            ));
        }
        fs::write(self.root.join("MEMORY.md"), idx).await?;
        Ok(())
    }
}

fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
        .collect()
}

fn parse_entry(raw: &str) -> Option<MemoryEntry> {
    let parts: Vec<&str> = raw.splitn(3, "---").collect();
    if parts.len() < 3 {
        return None;
    }
    let fm = parts[1];
    let body = parts[2].trim().to_string();
    let mut e = MemoryEntry {
        name: String::new(),
        kind: String::new(),
        tags: vec![],
        description: String::new(),
        body,
    };
    for line in fm.lines() {
        if let Some((k, v)) = line.split_once(':') {
            match k.trim() {
                "name" => e.name = v.trim().into(),
                "type" => e.kind = v.trim().into(),
                "description" => e.description = v.trim().into(),
                _ => {}
            }
        }
    }
    Some(e)
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().with_env_filter("info").init();
    let tmp = tempfile::tempdir()?;
    let store = MarkdownMemoryStore::new(tmp.path());

    store.save(&MemoryEntry {
        name: "user_role".into(),
        kind: "user".into(),
        tags: vec!["profile".into()],
        description: "用户角色与技术栈".into(),
        body: "用户是 Rust + AI Agent 方向的高级工程师，当前在做 harness engineering。".into(),
    }).await?;

    store.save(&MemoryEntry {
        name: "project_context".into(),
        kind: "project".into(),
        tags: vec!["mcc".into()],
        description: "mini-claude-code 项目当前目标".into(),
        body: "正在做 Rust 版 Claude Code，目标 2026-Q2 发布 0.1 到 crates.io。".into(),
    }).await?;

    let entries = store.load_all().await?;
    println!("Loaded {} memory entries:", entries.len());
    for e in entries {
        println!("- [{}] ({}) {}", e.kind, e.name, e.description);
    }

    println!("\n=== MEMORY.md ===\n{}", store.index_for_prompt().await?);
    Ok(())
}
