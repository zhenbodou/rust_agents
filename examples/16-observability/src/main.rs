//! 第 15 章：可观测性 —— JSON 日志 + 脱敏 + 成本追踪。

use std::collections::HashMap;

pub fn init(service: &str) {
    use tracing_subscriber::{fmt, prelude::*, EnvFilter};
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,agent=debug"));
    let layer = fmt::layer()
        .json()
        .with_current_span(true)
        .with_span_list(false)
        .with_target(true);
    tracing_subscriber::registry().with(filter).with(layer).init();
    tracing::info!(service = service, "observability initialized");
}

pub fn sanitize(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::String(s) if s.len() > 500 => {
            serde_json::Value::String(format!("{}…[len={}]", &s[..200], s.len()))
        }
        serde_json::Value::Object(m) => serde_json::Value::Object(
            m.iter()
                .map(|(k, v)| {
                    let kv = k.to_lowercase();
                    if ["password", "secret", "token", "key", "authorization", "apikey"]
                        .iter()
                        .any(|x| kv.contains(x))
                    {
                        (k.clone(), serde_json::Value::String("***".into()))
                    } else {
                        (k.clone(), sanitize(v))
                    }
                })
                .collect(),
        ),
        _ => value.clone(),
    }
}

#[derive(Debug, Default, Clone)]
pub struct CostTracker {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub model_mix: HashMap<String, u64>,
}

impl CostTracker {
    pub fn add(
        &mut self,
        model: &str,
        input: u32,
        output: u32,
        cache_read: u32,
        cache_write: u32,
    ) {
        self.input_tokens += input as u64;
        self.output_tokens += output as u64;
        self.cache_read += cache_read as u64;
        self.cache_write += cache_write as u64;
        *self
            .model_mix
            .entry(model.into())
            .or_insert(0) += (input + output) as u64;
    }

    /// Claude Opus 参考价格，实际以 Anthropic 当前官方为准。
    pub fn estimated_usd(&self) -> f64 {
        const OPUS_IN: f64 = 15.0 / 1_000_000.0;
        const OPUS_OUT: f64 = 75.0 / 1_000_000.0;
        const CACHE_READ: f64 = OPUS_IN * 0.1;
        const CACHE_WRITE: f64 = OPUS_IN * 1.25;
        self.input_tokens as f64 * OPUS_IN
            + self.output_tokens as f64 * OPUS_OUT
            + self.cache_read as f64 * CACHE_READ
            + self.cache_write as f64 * CACHE_WRITE
    }
}

fn main() {
    init("demo");

    let payload = serde_json::json!({
        "path": "/tmp/x",
        "apiKey": "sk-ant-very-secret",
        "nested": {"password": "hunter2", "ok": 1}
    });
    tracing::info!(target: "tool", args = %sanitize(&payload), "tool executed");

    let mut cost = CostTracker::default();
    cost.add("claude-opus-4-7", 8000, 500, 0, 8000);
    cost.add("claude-opus-4-7", 200, 300, 8000, 0);
    println!("cost: ${:.4}, tracker={:?}", cost.estimated_usd(), cost);
}
