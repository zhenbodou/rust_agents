# 第 17 章 错误处理、重试、限流与熔断

> Agent 的每个调用都在和不稳定的外界打交道：API 429、网络抖动、工具超时……本章把这些做成基础设施。

## 17.1 Rust 错误分层

```rust
#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error("LLM API error: status={status} body={body}")]
    Api { status: u16, body: String, retryable: bool },

    #[error("Rate limited; retry after {retry_after_ms}ms")]
    RateLimited { retry_after_ms: u64 },

    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("Timeout after {0:?}")]
    Timeout(std::time::Duration),

    #[error("Tool `{name}` error: {msg}")]
    Tool { name: String, msg: String },

    #[error("Budget exceeded: {0}")]
    Budget(String),

    #[error("Permission denied: {0}")]
    Permission(String),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl AgentError {
    pub fn is_retryable(&self) -> bool {
        match self {
            AgentError::Api { retryable, .. } => *retryable,
            AgentError::RateLimited { .. } => true,
            AgentError::Network(e) => e.is_timeout() || e.is_connect(),
            AgentError::Timeout(_) => true,
            _ => false,
        }
    }
}
```

**分类准则**：

| 类别 | 可重试 | 如何处理 |
|---|---|---|
| 5xx、502/503/504 | ✅ | 指数退避重试 |
| 429 | ✅ | 按 `retry-after` 头 sleep |
| 400 / 参数错 | ❌ | 立即失败，bug 信号 |
| 401 / 认证 | ❌ | 立即失败 |
| 工具 error | ❌（客户端） | 交给模型下一轮决定 |
| 网络 timeout | ✅ | 指数退避 |

## 17.2 带抖动的指数退避

```rust
use rand::Rng;
use std::time::Duration;

pub struct RetryPolicy {
    pub max_attempts: u32,
    pub base: Duration,       // 50ms
    pub cap: Duration,         // 30s
}

impl RetryPolicy {
    pub fn delay(&self, attempt: u32) -> Duration {
        let exp = self.base.as_millis() as u64 * (1u64 << attempt.min(10));
        let capped = exp.min(self.cap.as_millis() as u64);
        let jitter = rand::thread_rng().gen_range(0..=capped / 2);
        Duration::from_millis(capped / 2 + jitter)
    }
}

pub async fn with_retry<T, F, Fut>(policy: &RetryPolicy, mut f: F) -> Result<T, AgentError>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, AgentError>>,
{
    let mut attempt = 0;
    loop {
        match f().await {
            Ok(v) => return Ok(v),
            Err(e) if attempt < policy.max_attempts && e.is_retryable() => {
                let wait = match &e {
                    AgentError::RateLimited { retry_after_ms } => Duration::from_millis(*retry_after_ms),
                    _ => policy.delay(attempt),
                };
                tracing::warn!(attempt, ?wait, error = %e, "retrying");
                tokio::time::sleep(wait).await;
                attempt += 1;
            }
            Err(e) => return Err(e),
        }
    }
}
```

## 17.3 解析 Anthropic 错误

```rust
async fn parse_response(resp: reqwest::Response) -> Result<MessageResponse, AgentError> {
    let status = resp.status();
    let retry_after = resp.headers().get("retry-after")
        .and_then(|v| v.to_str().ok()).and_then(|s| s.parse().ok());
    let body = resp.text().await.unwrap_or_default();

    if status.is_success() {
        return serde_json::from_str(&body)
            .map_err(|e| AgentError::Other(anyhow::anyhow!("parse: {e}, body={body}")));
    }

    match status.as_u16() {
        429 => Err(AgentError::RateLimited { retry_after_ms: retry_after.unwrap_or(1000) * 1000 }),
        500..=599 => Err(AgentError::Api { status: status.as_u16(), body, retryable: true }),
        _ => Err(AgentError::Api { status: status.as_u16(), body, retryable: false }),
    }
}
```

## 17.4 限流：令牌桶客户端

避免我方主动打爆 API。用 [`governor`](https://crates.io/crates/governor)：

```rust
use governor::{Quota, RateLimiter, clock::DefaultClock, state::InMemoryState, state::NotKeyed};
use nonzero_ext::nonzero;
use std::sync::Arc;

pub struct ThrottledLlm {
    inner: Arc<dyn LlmProvider>,
    rps: RateLimiter<NotKeyed, InMemoryState, DefaultClock>,
    concurrency: tokio::sync::Semaphore,
}

impl ThrottledLlm {
    pub fn new(inner: Arc<dyn LlmProvider>, rpm: u32, max_concurrency: usize) -> Self {
        let quota = Quota::per_minute(std::num::NonZeroU32::new(rpm.max(1)).unwrap());
        Self {
            inner,
            rps: RateLimiter::direct(quota),
            concurrency: tokio::sync::Semaphore::new(max_concurrency),
        }
    }
}

#[async_trait::async_trait]
impl LlmProvider for ThrottledLlm {
    async fn complete(&self, req: CompleteRequest) -> anyhow::Result<MessageResponse> {
        let _permit = self.concurrency.acquire().await?;
        self.rps.until_ready().await;
        self.inner.complete(req).await
    }
    // stream 同理
}
```

## 17.5 熔断器

连续失败达到阈值时快速失败，给下游喘息：

```rust
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

pub struct CircuitBreaker {
    failure_threshold: u32,
    reset_timeout: Duration,
    failures: AtomicU32,
    opened_at: AtomicU64,        // epoch ms，0 表示关闭
}

impl CircuitBreaker {
    pub fn new(failure_threshold: u32, reset_timeout: Duration) -> Self {
        Self { failure_threshold, reset_timeout, failures: 0.into(), opened_at: 0.into() }
    }

    pub fn check(&self) -> Result<(), AgentError> {
        let opened = self.opened_at.load(Ordering::Relaxed);
        if opened == 0 { return Ok(()); }
        let now = now_ms();
        if now - opened > self.reset_timeout.as_millis() as u64 {
            self.opened_at.store(0, Ordering::Relaxed);
            self.failures.store(0, Ordering::Relaxed);
            Ok(())
        } else {
            Err(AgentError::Other(anyhow::anyhow!("circuit open")))
        }
    }

    pub fn record_success(&self) { self.failures.store(0, Ordering::Relaxed); }

    pub fn record_failure(&self) {
        let c = self.failures.fetch_add(1, Ordering::Relaxed) + 1;
        if c >= self.failure_threshold {
            self.opened_at.store(now_ms(), Ordering::Relaxed);
            tracing::error!(failures = c, "circuit opened");
        }
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64
}
```

## 17.6 超时策略

三层超时：

| 层 | 时长 | 作用 |
|---|---|---|
| 单次 HTTP | 120s | `reqwest::Client::timeout` |
| 单个 tool | 30–300s | `tokio::time::timeout` |
| 整个 turn | 5–10 min | agent loop 外层 wrap |

严格遵守：**外层 > 内层之和** ，否则会被内层 cancel 前外层先超时，难以定位。

## 17.7 故障注入测试

开发期主动注入失败验证重试行为：

```rust
pub struct FlakyProvider { inner: Arc<dyn LlmProvider>, failure_rate: f32 }

#[async_trait::async_trait]
impl LlmProvider for FlakyProvider {
    async fn complete(&self, req: CompleteRequest) -> anyhow::Result<MessageResponse> {
        if rand::random::<f32>() < self.failure_rate {
            return Err(AgentError::Api { status: 503, body: "injected".into(), retryable: true }.into());
        }
        self.inner.complete(req).await
    }
}
```

用环境变量 `AGENT_CHAOS=0.1` 控制。

## 17.8 小结

- 错误分类决定可否重试
- 带抖动的指数退避 + 尊重 `retry-after`
- 限流 + 熔断 + 三层超时 = 可靠调用基座
- 故障注入是保命测试

> **下一章**：Evals —— 怎么证明你的 Agent 真的变好了。

