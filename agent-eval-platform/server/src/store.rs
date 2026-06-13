//! 轨迹存储：append-only JSONL，后端可以是本地文件系统（开发）或 S3/MinIO（生产）。
//!
//! 设计原则（ch49）：
//!   - 完整轨迹永不进 Postgres——PG 只存索引/摘要，对象存储存原始数据
//!   - 写入 append-only，绝不覆盖；单条轨迹可能达 100MB+
//!   - 读取支持分页（offset/limit 按事件行数），进行中的 run 也可读
//!   - 后端可随时从 local 切换到 S3，接口不变
//!
//! 配置：
//!   TRACE_BACKEND=local  → 本地 TRACE_DIR 目录（默认）
//!   TRACE_BACKEND=s3     → S3 或 MinIO，需要 S3_* 环境变量

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use bytes::Bytes;
use object_store::path::Path as ObjPath;
use object_store::{ObjectStore, PutMode, PutOptions, PutPayload};
use uuid::Uuid;

// ─── public types ────────────────────────────────────────────────────────────

#[derive(serde::Serialize)]
pub struct TracePage {
    pub events: Vec<serde_json::Value>,
    pub total: usize,
    pub next_offset: Option<usize>,
}

// ─── store facade ────────────────────────────────────────────────────────────

/// 轨迹存储门面。Clone 是廉价的（Arc 内部）。
#[derive(Clone)]
pub struct TraceStore {
    inner: Arc<dyn ObjectStore>,
    prefix: String, // S3: bucket 内 prefix；local: 不用
}

impl TraceStore {
    /// 从环境变量初始化。
    ///
    /// 本地：`TRACE_BACKEND=local TRACE_DIR=./traces`
    /// MinIO：`TRACE_BACKEND=s3 S3_ENDPOINT=http://minio:9000
    ///         S3_BUCKET=traces S3_ACCESS_KEY=… S3_SECRET_KEY=…`
    pub fn from_env() -> Result<Self> {
        let backend = std::env::var("TRACE_BACKEND").unwrap_or_else(|_| "local".into());
        match backend.as_str() {
            "s3" | "minio" => Self::new_s3(),
            _ => Self::new_local(),
        }
    }

    fn new_local() -> Result<Self> {
        let dir = std::env::var("TRACE_DIR").unwrap_or_else(|_| "./traces".into());
        let path = PathBuf::from(&dir);
        std::fs::create_dir_all(&path)
            .with_context(|| format!("create trace dir {dir}"))?;
        let store = object_store::local::LocalFileSystem::new_with_prefix(&path)?;
        tracing::info!(dir, "TraceStore: local filesystem");
        Ok(Self {
            inner: Arc::new(store),
            prefix: String::new(),
        })
    }

    fn new_s3() -> Result<Self> {
        let endpoint = std::env::var("S3_ENDPOINT")
            .unwrap_or_else(|_| "http://minio:9000".into());
        let bucket = std::env::var("S3_BUCKET").unwrap_or_else(|_| "traces".into());
        let access_key = std::env::var("S3_ACCESS_KEY")
            .context("S3_ACCESS_KEY required for s3 backend")?;
        let secret_key = std::env::var("S3_SECRET_KEY")
            .context("S3_SECRET_KEY required for s3 backend")?;
        let region = std::env::var("S3_REGION").unwrap_or_else(|_| "us-east-1".into());

        let store = object_store::aws::AmazonS3Builder::new()
            .with_endpoint(endpoint.clone())
            .with_bucket_name(bucket.clone())
            .with_access_key_id(access_key)
            .with_secret_access_key(secret_key)
            .with_region(region)
            // MinIO 要求路径寻址而非虚拟主机寻址
            .with_virtual_hosted_style_request(false)
            .build()
            .context("build S3 store")?;

        tracing::info!(endpoint, bucket, "TraceStore: S3/MinIO");
        Ok(Self {
            inner: Arc::new(store),
            prefix: String::new(),
        })
    }

    /// 测试专用：在系统临时目录创建隔离的本地存储
    #[cfg(test)]
    pub fn new_local_tmp() -> Self {
        let dir = std::env::temp_dir().join(format!("eval-traces-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = object_store::local::LocalFileSystem::new_with_prefix(&dir).unwrap();
        Self {
            inner: Arc::new(store),
            prefix: String::new(),
        }
    }

    // ── 内部帮助 ──────────────────────────────────────────────────────────

    fn key(&self, run_id: Uuid) -> ObjPath {
        if self.prefix.is_empty() {
            ObjPath::from(format!("{run_id}.jsonl"))
        } else {
            ObjPath::from(format!("{}/{run_id}.jsonl", self.prefix))
        }
    }

    // ── 写 ──────────────────────────────────────────────────────────────

    /// 追加一批 JSONL 行。
    ///
    /// S3 不支持原生 append，用"读 + 追加 + 写回"模式。
    /// 对进行中的 run 这会产生写放大；生产优化：分片写入（每 N 秒一个 part），
    /// finalize 时用 S3 multipart complete 合并。教学版保持接口简洁，trade-off 已说清。
    pub async fn append(&self, run_id: Uuid, lines: &[&str]) -> Result<()> {
        if lines.is_empty() {
            return Ok(());
        }
        let key = self.key(run_id);
        let new_bytes: Vec<u8> = lines
            .iter()
            .flat_map(|l| [l.as_bytes(), b"\n" as &[u8]])
            .flatten()
            .copied()
            .collect();

        // 读取已有内容（首次写不存在，正常）
        let existing: Vec<u8> = match self.inner.get(&key).await {
            Ok(result) => result.bytes().await?.to_vec(),
            Err(object_store::Error::NotFound { .. }) => Vec::new(),
            Err(e) => return Err(e.into()),
        };

        let mut merged = existing;
        merged.extend_from_slice(&new_bytes);

        self.inner
            .put_opts(
                &key,
                PutPayload::from_bytes(Bytes::from(merged)),
                PutOptions {
                    mode: PutMode::Overwrite,
                    ..Default::default()
                },
            )
            .await
            .context("put trace object")?;
        Ok(())
    }

    /// run 完成：返回对象键（写进 runs.trace_path）。
    pub fn finalize(&self, run_id: Uuid) -> String {
        self.key(run_id).to_string()
    }

    // ── 读 ──────────────────────────────────────────────────────────────

    /// 分页读（offset/limit 按事件行数）。进行中的 run 也可读。
    pub async fn read_page(
        &self,
        run_id: Uuid,
        offset: usize,
        limit: usize,
    ) -> Result<TracePage> {
        let key = self.key(run_id);
        let content = match self.inner.get(&key).await {
            Ok(result) => {
                let bytes = result.bytes().await?;
                String::from_utf8_lossy(&bytes).into_owned()
            }
            Err(object_store::Error::NotFound { .. }) => String::new(),
            Err(e) => return Err(e.into()),
        };

        let all: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();
        let total = all.len();
        let events: Vec<serde_json::Value> = all
            .iter()
            .skip(offset)
            .take(limit)
            // 损坏行（进程被杀写了半行）跳过而非崩溃（ch38 原则）
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect();
        let next_offset = (offset + limit < total).then_some(offset + limit);
        Ok(TracePage {
            events,
            total,
            next_offset,
        })
    }

    /// 对象键（写进 runs.trace_path，用于日后下载）
    pub fn object_key(&self, run_id: Uuid) -> String {
        self.key(run_id).to_string()
    }
}

// ─── 健康检查 ─────────────────────────────────────────────────────────────

impl TraceStore {
    /// 写/读一个测试对象，验证后端连通性（/healthz/ready 调用）。
    pub async fn health_check(&self) -> Result<()> {
        let key = ObjPath::from("_healthcheck");
        self.inner
            .put(&key, PutPayload::from_static(b"ok"))
            .await
            .context("trace store health check write")?;
        let _ = self.inner.delete(&key).await;
        Ok(())
    }
}
