# agent-eval-platform

Agent 评测平台——本书第十部分（第 49–52 章）的配套实战项目。一个真实可跑的全栈系统：

```
React (web/)  ──HTTP/SSE──►  Rust axum (server/)  ──SQL──►  PostgreSQL
   轨迹回放                    │  REST + SSE hub                元数据/索引
   批次看板                    │  租约调度 + reaper
                              └──JSONL──► 轨迹文件 (开发态；生产换 S3)
                                   ▲
Python (runner/) ──lease/events/complete──┘
   mock / anthropic 适配器（统一 TraceEvent 契约，书 ch42）
```

## 快速开始（无需任何 API key）

```bash
docker compose up --build        # db + server + 2×runner + web
./scripts/demo-batch.sh          # 提交 5 个演示 case（mock agent）
open http://localhost:5173       # 看板 → 点进批次 → 点进 run 看实时轨迹
```

mock agent 会模拟"grep → edit → cargo test"三轮执行并流式产出事件；任务文本含 "fail" 的 case 会失败——刻意如此，用来演示失败过滤与对比报告。

### 不用 Docker 的本地开发

```bash
# 1. Postgres
docker run -d -p 5432:5432 -e POSTGRES_PASSWORD=dev -e POSTGRES_DB=evalplatform postgres:17-alpine
# 2. 后端（自动跑迁移）
cd server && cargo run
# 3. runner
cd runner && uv sync && uv run python -m runner.main
# 4. 前端（Vite 代理 /api → 8080）
cd web && pnpm install && pnpm dev
```

### 接真实模型

```bash
cd runner && uv sync --extra anthropic
export ANTHROPIC_API_KEY=sk-ant-...
# 注册一个 anthropic profile（也可直接 INSERT agent_profiles）：
psql "$DATABASE_URL" -c "INSERT INTO agent_profiles (name, scaffold, model)
  VALUES ('claude-bash', 'anthropic', 'claude-sonnet-4-6');"
# 提交批次时 "profile": "claude-bash"
```

## 目录与对应章节

| 目录 | 内容 | 章节 |
|---|---|---|
| `schemas/` | TraceEvent JSON Schema（三端单一事实来源） | ch49.4 |
| `server/` | axum API、SSE hub、`FOR UPDATE SKIP LOCKED` 租约、reaper、轨迹存储 | ch50 |
| `runner/` | lease 主循环、心跳、批量上报、mock/anthropic 适配器 | ch50/ch42 |
| `web/` | 批次看板、Trace Viewer（历史+SSE 按 seq 合并）、未知事件降级 | ch51 |
| `docker-compose.yaml` | 本地一键全栈 | ch52.1 |
| `deploy/k8s/` | Kustomize base + overlays、探针、PDB、SSE Ingress 配置 | ch52.2 |
| `.github/workflows/` | 三栈 CI + 镜像构建 | ch48 |

## 刻意保留的"本地捷径"（生产差异清单，书 52.1）

| 此项目（教学） | 生产 |
|---|---|
| 轨迹存本地文件 | S3/OSS multipart + zstd + lifecycle |
| `/internal` 无鉴权 | mTLS 或 service token 中间件 |
| mock/anthropic 工具在 runner 进程内执行 | 工具收归沙箱服务（gVisor pod，ch47） |
| CORS permissive | 域名白名单 |
| K8s 用 emptyDir + 演示级 PG | PVC/云盘 + 托管数据库 + External Secrets |

每一条都是书中练习：把它升级到生产形态，就是你的作品集素材。

## 测试

```bash
cd server && cargo test          # 含 schema 契约测试
cd runner && uv run pytest       # 含 schema 契约测试（与 Rust 端互锁）
cd web && pnpm tsc --noEmit
```

## 建议的扩展练习（按难度递增）

1. **对比页**：后端 `/api/reports/compare` 已就绪，给 web 加 `/compare?a=&b=` 页面（书 51.4）。
2. **虚拟列表**：轨迹超 2000 事件时换 @tanstack/react-virtual（书 38.3）。
3. **internal 鉴权**：给 `/internal/*` 加 Bearer token 中间件 + runner 侧配置。
4. **S3 TraceStore**：用 MinIO 实现第二个 TraceStore（接口已对齐），compose 里加 minio 服务。
5. **KEDA 扩缩**：prod overlay 补 ScaledObject，按 `queued` 数扩 runner、夜间缩 0（书 46.5）。
6. **金丝雀门禁**：写 canary-gate 脚本 + GitHub Actions job，staging 跑固定任务集、通过率跌 2% 阻断（书 52.3）。
7. **mini-claude-code 适配器**：写第三个 adapter，以子进程 JSONL 方式驱动本仓库的 mcc（书 41.5 互操作模式 2）。
8. **RL 轨迹导出**：给 runner 加 `--training-format` 模式，输出 ch43 的 TrainingTrajectory。
