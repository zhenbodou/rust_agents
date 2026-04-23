# 第 15 章 可观测性：日志、Trace、Metrics

> 没有可观测性的 Agent 上线就是黑盒，每次事故都要从头猜。本章教你一次做对。

## 15.1 可观测性的三支柱

| 名字 | 回答的问题 | 技术 |
|---|---|---|
| Logs | **发生了什么** | `tracing`, JSON 日志 |
| Traces | **调用链路和耗时** | OpenTelemetry |
| Metrics | **趋势与聚合** | Prometheus, OTel |

对 Agent 还要加第四支柱：

| Replays | **同一 session 能否复现** | 持久化 messages + tool outputs |

## 15.2 结构化日志

Rust 的 `tracing` 生态几乎是行业标准。

```rust
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

pub fn init(service_name: &str) {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,agent=debug"));

    // 开发用 pretty，生产用 JSON
    let json_layer = fmt::layer()
        .json()
        .with_current_span(true)
        .with_span_list(false)
        .with_target(true);

    tracing_subscriber::registry()
        .with(filter)
        .with(json_layer)
        .init();

    tracing::info!(service = service_name, "observability initialized");
}
```

### 关键字段约定

强制所有 Agent 相关日志带上：

```rust
tracing::info!(
    session_id = %ctx.session_id,
    iteration = iter,
    model = %model,
    tool = %tool_name,
    latency_ms = elapsed.as_millis() as u64,
    input_tokens = usage.input_tokens,
    output_tokens = usage.output_tokens,
    "llm call completed"
);
```

### 敏感字段脱敏

绝不要直接把 user prompt / tool input 落日志。做一个过滤 layer：

```rust
pub fn sanitize(value: &serde_json::Value) -> serde_json::Value {
    // 简化：把长字符串截断；匹配到 "password"/"token"/"key" 字段置 ***
    match value {
        serde_json::Value::String(s) if s.len() > 500 => serde_json::Value::String(format!("{}…[len={}]", &s[..200], s.len())),
        serde_json::Value::Object(m) => serde_json::Value::Object(m.iter().map(|(k, v)| {
            let kv = k.to_lowercase();
            if ["password","secret","token","key","authorization","apikey"].iter().any(|x| kv.contains(x)) {
                (k.clone(), serde_json::Value::String("***".into()))
            } else {
                (k.clone(), sanitize(v))
            }
        }).collect()),
        _ => value.clone()
    }
}
```

## 15.3 OpenTelemetry Traces

Agent 的一次 turn 应该生成一棵 trace：

```text
turn (root span)
├─ llm.complete (attrs: model, input_tokens, output_tokens)
├─ tool.execute name=read_file  (attrs: path, bytes)
├─ tool.execute name=run_bash   (attrs: cmd, exit_code, duration)
└─ llm.complete                 (second iteration)
```

### Rust OTel 接线

```toml
[dependencies]
opentelemetry = "0.24"
opentelemetry_sdk = { version = "0.24", features = ["rt-tokio"] }
opentelemetry-otlp = { version = "0.17", features = ["tonic"] }
tracing-opentelemetry = "0.25"
```

```rust
use opentelemetry::global;
use opentelemetry_sdk::{runtime, trace::TracerProvider};
use opentelemetry_otlp::WithExportConfig;

pub fn init_tracing_otel(service_name: &str) -> anyhow::Result<()> {
    global::set_text_map_propagator(opentelemetry_sdk::propagation::TraceContextPropagator::new());

    let exporter = opentelemetry_otlp::new_exporter().tonic().with_endpoint(
        std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").unwrap_or_else(|_| "http://localhost:4317".into()),
    );

    let tracer_provider = opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(exporter)
        .with_trace_config(
            opentelemetry_sdk::trace::Config::default().with_resource(
                opentelemetry_sdk::Resource::new(vec![
                    opentelemetry::KeyValue::new("service.name", service_name.to_string()),
                ]),
            ),
        )
        .install_batch(runtime::Tokio)?;

    let tracer = tracer_provider.tracer("agent");
    let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);

    tracing_subscriber::registry()
        .with(EnvFilter::from_default_env())
        .with(tracing_subscriber::fmt::layer().json())
        .with(otel_layer)
        .init();
    Ok(())
}
```

### Span 使用

```rust
use tracing::Instrument;

let span = tracing::info_span!("llm.complete", model = %req.model);
let resp = self.http.post(...).send().instrument(span.clone()).await?;

span.record("input_tokens", resp.usage.input_tokens);
span.record("output_tokens", resp.usage.output_tokens);
```

## 15.4 Metrics

用 `metrics` + `metrics-exporter-prometheus` 暴露 `/metrics`。

```rust
use metrics::{counter, histogram};

pub fn record_llm_call(model: &str, input_tokens: u32, output_tokens: u32, latency_ms: u64) {
    counter!("agent_llm_calls_total", "model" => model.to_string()).increment(1);
    histogram!("agent_llm_input_tokens", "model" => model.to_string()).record(input_tokens as f64);
    histogram!("agent_llm_output_tokens", "model" => model.to_string()).record(output_tokens as f64);
    histogram!("agent_llm_latency_ms", "model" => model.to_string()).record(latency_ms as f64);
}
```

推荐指标清单：

| 指标 | 类型 | 标签 |
|---|---|---|
| `agent_llm_calls_total` | Counter | model, stop_reason |
| `agent_llm_latency_ms` | Histogram | model |
| `agent_llm_tokens_total` | Counter | model, kind(input/output/cache_read/cache_write) |
| `agent_tool_calls_total` | Counter | tool, is_error |
| `agent_tool_duration_ms` | Histogram | tool |
| `agent_permission_decisions_total` | Counter | category, decision |
| `agent_loop_iterations` | Histogram | session_id_hash |
| `agent_active_sessions` | Gauge | — |

## 15.5 Session Replays

企业级 Agent 必须能"复放"一个 session 定位问题。最小实现：每轮把完整 `messages + usage + tool outputs` 按 JSONL 追加写入：

```rust
pub struct SessionRecorder { file: tokio::fs::File }

impl SessionRecorder {
    pub async fn record_turn(&mut self, turn: &TurnSnapshot) -> anyhow::Result<()> {
        use tokio::io::AsyncWriteExt;
        let line = serde_json::to_string(turn)?;
        self.file.write_all(line.as_bytes()).await?;
        self.file.write_all(b"\n").await?;
        Ok(())
    }
}

#[derive(serde::Serialize)]
pub struct TurnSnapshot {
    pub ts: String,
    pub session_id: String,
    pub iteration: u32,
    pub request_messages: Vec<Message>,
    pub response: Vec<ContentBlock>,
    pub tool_results: Vec<(String, String, bool)>,
    pub usage: Usage,
}
```

Replay 工具：读 JSONL → 重跑（可选地用 mock LLM 返回记录的 response）。这也是 evals 的原料（第 18 章）。

## 15.6 成本 Dashboard

每个 session 的成本汇总是产品 KPI：

```rust
#[derive(Debug, Default)]
pub struct CostTracker {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub model_mix: HashMap<String, u64>,
}

impl CostTracker {
    pub fn add(&mut self, model: &str, u: Usage) {
        self.input_tokens += u.input_tokens as u64;
        self.output_tokens += u.output_tokens as u64;
        self.cache_read += u.cache_read_input_tokens as u64;
        self.cache_write += u.cache_creation_input_tokens as u64;
        *self.model_mix.entry(model.into()).or_insert(0) += (u.input_tokens + u.output_tokens) as u64;
    }

    pub fn estimated_usd(&self) -> f64 {
        // 示例价格（实际请查 Anthropic pricing）
        const OPUS_IN:  f64 = 15.0 / 1_000_000.0;
        const OPUS_OUT: f64 = 75.0 / 1_000_000.0;
        const CACHE_READ: f64 = OPUS_IN * 0.1;
        (self.input_tokens as f64) * OPUS_IN
            + (self.output_tokens as f64) * OPUS_OUT
            + (self.cache_read as f64) * CACHE_READ
    }
}
```

## 15.7 推荐的栈

开发机：`tracing-subscriber fmt` + 本地 Jaeger/Tempo + Prometheus + Grafana
生产：OTel Collector → Tempo / Honeycomb / Datadog；日志 → Loki / ELK；指标 → Prometheus / VictoriaMetrics。

**Honeycomb / LangSmith / Helicone** 这类 LLM-专用 observability 平台已经不少，生产项目不妨直接对接以省自建成本。

## 15.8 小结

- 4 支柱：Logs / Traces / Metrics / Replays
- 每个 LLM 调用、每个 tool 调用都是 span
- 成本追踪是 Agent 产品的必备仪表
- 敏感字段脱敏不是可选项

> **下一章**：Prompt Caching —— 降本 90% 的关键武器。

