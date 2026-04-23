//! 第 17 章：错误分层 + 带抖动重试 + 熔断器。

use rand::Rng;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AgentError {
    #[error("api error status={status} body={body}")]
    Api { status: u16, body: String, retryable: bool },
    #[error("rate limited, retry after {retry_after_ms}ms")]
    RateLimited { retry_after_ms: u64 },
    #[error("timeout after {0:?}")]
    Timeout(Duration),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl AgentError {
    pub fn is_retryable(&self) -> bool {
        match self {
            AgentError::Api { retryable, .. } => *retryable,
            AgentError::RateLimited { .. } => true,
            AgentError::Timeout(_) => true,
            _ => false,
        }
    }
}

pub struct RetryPolicy {
    pub max_attempts: u32,
    pub base: Duration,
    pub cap: Duration,
}

impl RetryPolicy {
    pub fn delay(&self, attempt: u32) -> Duration {
        let exp = self.base.as_millis() as u64 * (1u64 << attempt.min(10));
        let capped = exp.min(self.cap.as_millis() as u64);
        let jitter: u64 = rand::thread_rng().gen_range(0..=capped / 2);
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
                    AgentError::RateLimited { retry_after_ms } => {
                        Duration::from_millis(*retry_after_ms)
                    }
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

pub struct CircuitBreaker {
    failure_threshold: u32,
    reset_timeout: Duration,
    failures: AtomicU32,
    opened_at: AtomicU64,
}

impl CircuitBreaker {
    pub fn new(failure_threshold: u32, reset_timeout: Duration) -> Self {
        Self {
            failure_threshold,
            reset_timeout,
            failures: AtomicU32::new(0),
            opened_at: AtomicU64::new(0),
        }
    }

    pub fn check(&self) -> Result<(), AgentError> {
        let opened = self.opened_at.load(Ordering::Relaxed);
        if opened == 0 {
            return Ok(());
        }
        let now = now_ms();
        if now - opened > self.reset_timeout.as_millis() as u64 {
            self.opened_at.store(0, Ordering::Relaxed);
            self.failures.store(0, Ordering::Relaxed);
            Ok(())
        } else {
            Err(AgentError::Other(anyhow::anyhow!("circuit open")))
        }
    }

    pub fn record_success(&self) {
        self.failures.store(0, Ordering::Relaxed);
    }

    pub fn record_failure(&self) {
        let c = self.failures.fetch_add(1, Ordering::Relaxed) + 1;
        if c >= self.failure_threshold {
            self.opened_at.store(now_ms(), Ordering::Relaxed);
            tracing::error!(failures = c, "circuit opened");
        }
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_env_filter("info").init();

    let policy = RetryPolicy {
        max_attempts: 4,
        base: Duration::from_millis(50),
        cap: Duration::from_secs(2),
    };

    let attempts = std::sync::atomic::AtomicU32::new(0);
    let result = with_retry(&policy, || async {
        let n = attempts.fetch_add(1, Ordering::Relaxed);
        if n < 2 {
            Err(AgentError::Api {
                status: 503,
                body: "transient".into(),
                retryable: true,
            })
        } else {
            Ok(format!("ok after {} attempts", n + 1))
        }
    })
    .await;

    println!("result: {result:?}");

    let cb = CircuitBreaker::new(3, Duration::from_secs(5));
    for _ in 0..3 {
        cb.record_failure();
    }
    assert!(cb.check().is_err());
    println!("circuit is open as expected");

    Ok(())
}
