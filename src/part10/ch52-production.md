# 第 52 章 部署上线与生产运维

> 代码写完只是一半，让它在生产环境里活下来是另一半。本章按"本地 → 集群 → 持续交付 → 运维"的顺序走完最后一公里，把 Part 9 学的容器、K8s、CI/CD 全用上。走完这章，你拥有一个从开发到上线完整闭环的作品集项目。

## 52.1 本地：六服务全栈 `docker compose up`

`docker-compose.yaml` 在本地把整套服务起起来，包含 MinIO、Prometheus、Grafana：

```
服务清单：
  db          PostgreSQL 17
  minio       S3 兼容对象存储（轨迹文件）
  minio-init  一次性任务：创建 traces bucket
  server      Rust API 服务
  runner      Python 评测 runner（可 --scale runner=4）
  web         React 前端
  prometheus  指标采集
  grafana     指标可视化（admin/admin）
```

几个值得关注的配置细节：

**服务健康检查链**：`minio-init` 等 minio `healthy` 后才跑（建 bucket），`server` 等 `minio-init` 完成 + `db` 健康后才起，`runner` 等 `server healthy` 后才连。这样 `docker compose up` 是幂等的，任意顺序重启都不会出现"先连后建"的竞态。

```yaml
server:
  depends_on:
    db:         { condition: service_healthy }
    minio-init: { condition: service_completed_successfully }
```

**`RUNNER_SECRET` 通过环境变量注入**：

```bash
# .env（本地开发）
RUNNER_SECRET=dev-runner-secret
```

Server 和 Runner 的 compose 里都引用 `${RUNNER_SECRET:-dev-runner-secret}`，`:-` 给默认值，开发不用必须设，上生产换真 secret。

**Runner 横向扩展一行命令**：

```bash
docker compose up --scale runner=4
```

四个 Runner 并发领任务，`FOR UPDATE SKIP LOCKED` 保证绝不重复分配。这是验证调度正确性的最快方式——直接看日志里的 `runner_id` 分布是否均匀。

**本地和生产的三处刻意不同**：

| | 本地（图省事）| 生产（要安全）|
|---|---|---|
| 对象存储 | MinIO | 真 S3 + IAM Role（不用 Access Key）|
| 密钥 | `${RUNNER_SECRET:-dev-runner-secret}` | Kubernetes Secret + 外部 Vault 注入 |
| Grafana 密码 | `admin` | 强密码 + SSO |

这些差异必须在 README 里明确标注——新人最容易把本地的安全捷径直接带进生产。

## 52.2 Prometheus + Grafana 接入

Prometheus 配置只需一个 `scrape_configs`：

```yaml
# deploy/prometheus.yml
scrape_configs:
  - job_name: eval-server
    static_configs:
      - targets: ["server:8080"]
    metrics_path: /metrics
```

Grafana 通过 provisioning 目录自动导入 Prometheus 数据源，不用手工点击：

```yaml
# deploy/grafana/provisioning/datasources/prometheus.yaml
datasources:
  - name: Prometheus
    type: prometheus
    url: http://prometheus:9090
    isDefault: true
```

**核心看板推荐建四张图**（Grafana Query 示例）：

```
通过率趋势：
  rate(eval_runs_total{status="passed"}[5m]) /
  rate(eval_runs_total[5m])

API 延迟 P99：
  histogram_quantile(0.99, rate(http_request_duration_seconds_bucket[5m]))

队列积压：
  eval_queue_depth

成本累积（按小时）：
  increase(eval_run_cost_usd_sum[1h])
```

## 52.3 部署到 K8s：Kustomize 管理环境差异

```
deploy/k8s/
├── base/
│   ├── server.yaml         Deployment + Service
│   ├── runner.yaml         Deployment（无 Service）
│   └── networkpolicy.yaml  沙箱默认断网
└── overlays/
    ├── staging/            1 副本、小资源、跳过 gVisor
    └── prod/               3 副本、HPA、PDB、gVisor
```

生产配置比 base 多三样关键东西：

**PodDisruptionBudget**：节点维护时也至少保留 2 个副本，服务不中断：

```yaml
kind: PodDisruptionBudget
spec:
  minAvailable: 2
  selector: { matchLabels: { app: eval-server } }
```

**基于队列深度的自动扩缩容（KEDA）**：

```yaml
kind: ScaledObject
spec:
  scaleTargetRef: { name: runner }
  minReplicaCount: 0    # 没任务时缩到 0，省钱
  maxReplicaCount: 50
  triggers:
    - type: postgresql
      metadata:
        query: "SELECT count(*) FROM runs WHERE status='queued'"
        targetQueryValue: "5"   # 每 5 个排队任务对应 1 个 Runner
```

这是整个调度设计的一个闭环：Runner 数量随队列深度自动伸缩，高峰扩、低谷缩，而不需要人工干预。

**Runner 最小权限 RBAC**：

```yaml
# runner 只能在 sandbox 命名空间创建/删除 pod，碰不到 secret 和其他命名空间
rules:
  - apiGroups: [""]
    resources: ["pods"]
    verbs: ["create","delete","get","list"]
    namespaces: ["sandbox"]
```

万一沙箱被攻破，爆炸半径就被 RBAC 框死在这个范围内。

## 52.4 CI/CD：测试 → 镜像 → 部署三段流水线

```yaml
# .github/workflows/ci.yml（概览）
jobs:
  test:
    - cargo test --test integration_test  # 起真实 Postgres + MinIO 容器
    - pytest runner/tests/               # Python runner 单元测试
    - cd web && npm test                 # 前端类型检查 + Vitest

  build:
    needs: test
    - docker build server/ → ghcr.io/org/eval-server:$SHA
    - docker build runner/ → ghcr.io/org/eval-runner:$SHA
    - docker build web/    → ghcr.io/org/eval-web:$SHA

  deploy:
    needs: build
    if: github.ref == 'refs/heads/main'
    - kustomize edit set image ... $SHA
    - kubectl apply -k deploy/k8s/overlays/prod/
    - kubectl rollout status deployment/eval-server
```

**集成测试是流水线里最贵的步骤**，但也是最值得的：它能发现 mock 测试发现不了的问题（`SKIP LOCKED` 并发行为、MinIO 协议细节、DB migration 破坏性变更）。流水线里加 `--test-threads=1` 让测试顺序执行，避免多个 Postgres 容器并发启动时端口冲突。

## 52.5 运维手册：高频故障排查

| 故障现象 | 第一步排查 | 典型原因 |
|---|---|---|
| 任务全部卡在 queued | `eval_queue_depth` 飙高，`eval_active_runs` 为 0 | Runner 全部挂掉 / 鉴权 token 不匹配 |
| 通过率突然下降 | 看最近批次的 error message | 模型 API 限速 / 沙箱 OOM |
| `/healthz/ready` 返回 500 | server 日志 `internal error` | PostgreSQL 断连 / MinIO 不可达 |
| Prometheus 没数据 | `curl server:8080/metrics` 手动检查 | scrape_interval 配错 / /metrics 路由未注册 |
| Runner 领不到任务 | server 日志看 401 | RUNNER_SECRET 环境变量忘配 |

**两个生产必装的告警规则**：

```yaml
# 队列积压超 5 分钟 → 通知 oncall
- alert: EvalQueueBacklog
  expr: eval_queue_depth > 50
  for: 5m

# 通过率连续 1 小时低于 60% → 通知算法团队
- alert: EvalPassRateLow
  expr: rate(eval_runs_total{status="passed"}[1h]) /
        rate(eval_runs_total[1h]) < 0.6
  for: 1h
```

## 52.6 全书收尾：从这里往哪里走

你现在拥有了一个**从端到端、从开发到上线**完整闭环的项目：

```
Rust 后端（Part 5）
  + PostgreSQL 分布式任务队列（Part 8）
  + Python runner + LangGraph 适配（Part 7/8）
  + React 前端 + recharts 可视化（Part 6）
  + testcontainers 集成测试（Part 9/10）
  + MinIO/S3 对象存储（object_store crate）
  + Prometheus/Grafana 可观测性（Part 9）
  + K8s + KEDA 自动扩缩（Part 9）
```

接下来三个方向，选最适合你目标的：

**如果要找工作**：把项目推到 GitHub，写好 README（是什么 / 为什么 / 本地怎么跑），录一个 3 分钟 demo 视频展示批次提交→实时轨迹→对比报告完整流程。这个项目本身就是最好的面试作品集。

**如果要在内部落地**：接下来的工作是接真实 Agent（换 scaffold）、写真实 test case（填 `expectations` 字段）、配 CI 触发（在 PR 流水线里 POST `/api/batches`）。平台本身不需要再改。

**如果要深入研究**：重点是 Runner 侧的评分（`scoring.py` 里的 `expectations` 格式设计）和 LangGraph 适配层（`langgraph_agent.py`）——这两块的质量决定了评测结果是否真的能指导模型/Agent 改进。

这就是全书最核心的一句话，在第一章说过、现在兑现了：

> **Harness Engineer 的价值不在于 Agent 本身，而在于让 Agent 可测、可比、可信。**

---

**全书练习（综合）**

1. 给 `scoring.py` 加一种新的 expectation 类型 `output_not_contains`（不能包含某字符串），写对应测试。
2. 在 CI 里加一个"成本回归检测"：如果这个 PR 让某个 benchmark case 的 `cost_usd` 超过了上次的 120%，自动在 PR 留评论提醒。
3. 给仪表盘加"最慢 10 个 case"排行榜，用于定向优化长尾延迟。
