//! Session recorder：JSONL 流式落盘 + 元数据。

use mcc_core::{ContentBlock, Message, Usage};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMeta {
    pub id: String,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub cwd: PathBuf,
    pub turns: u32,
    pub cost_usd: f64,
    pub status: String,
    pub title: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TurnSnapshot {
    pub ts: chrono::DateTime<chrono::Utc>,
    pub iteration: u32,
    pub request_messages: Vec<Message>,
    pub assistant_blocks: Vec<ContentBlock>,
    pub tool_outputs: Vec<(String, String, bool)>,
    pub usage: Usage,
    pub model: String,
}

pub struct SessionRecorder {
    pub id: String,
    writer: Mutex<tokio::fs::File>,
    meta_path: PathBuf,
    meta: std::sync::Mutex<SessionMeta>,
}

impl SessionRecorder {
    pub async fn open(id: &str, cwd: &Path) -> anyhow::Result<Self> {
        let base = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".mcc/sessions")
            .join(chrono::Utc::now().format("%Y-%m-%d").to_string());
        tokio::fs::create_dir_all(&base).await?;

        let jsonl = base.join(format!("{id}.jsonl"));
        let meta_path = base.join(format!("{id}.meta.json"));
        let file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&jsonl)
            .await?;

        let meta = SessionMeta {
            id: id.into(),
            started_at: chrono::Utc::now(),
            cwd: cwd.into(),
            turns: 0,
            cost_usd: 0.0,
            status: "running".into(),
            title: None,
        };
        tokio::fs::write(&meta_path, serde_json::to_string_pretty(&meta)?).await?;

        Ok(Self {
            id: id.into(),
            writer: Mutex::new(file),
            meta_path,
            meta: std::sync::Mutex::new(meta),
        })
    }

    pub async fn record(&self, turn: TurnSnapshot) -> anyhow::Result<()> {
        let line = serde_json::to_string(&turn)? + "\n";
        self.writer.lock().await.write_all(line.as_bytes()).await?;

        let snapshot = {
            let mut m = self.meta.lock().unwrap();
            m.turns += 1;
            m.clone()
        };
        tokio::fs::write(&self.meta_path, serde_json::to_string_pretty(&snapshot)?).await?;
        Ok(())
    }
}
