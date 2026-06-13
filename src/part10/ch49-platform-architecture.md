# 第 49 章 评测平台架构总览

> 终于到了把所有东西拼起来的时候。前面九部分你学了 Agent、harness、前端、后端、容器、运维——这一部分我们用它们造一个**真实可跑的企业级项目**：一个 Agent 评测平台。它能批量跑 Agent、实时看轨迹、对比不同版本、出统计报表。代码在仓库 `agent-eval-platform/` 目录。本章先把蓝图画清楚，后面三章逐块实现。

## 49.1 先当产品经理：谁用、用来干嘛

做任何系统，先想清楚"谁用、解决什么问题"，别一上来写代码。这个平台的用户和他们的高频需求：

| 用户 | 他想干嘛 | 对应功能 |
|---|---|---|
| 算法工程师 | "新版模型比上一版好吗？" | 提交评测、看对比报告 |
| 算法工程师 | "这个用例为什么挂了？" | 轨迹回放、逐轮详情 |
| Harness 工程师 | "我改的工具有没有让效果回退？" | 按版本分组的趋势图 |
| 团队 lead | "这周通过率和成本怎么样？" | 仪表盘 |
| CI 机器人 | "PR 合入前自动跑回归" | API 接口 |

同样重要的是想清楚**不做什么**：不做数据标注、不做模型托管、不做通用报表。内部工具最常见的死法就是"什么都想做"，最后什么都做不好。

## 49.2 总体架构：一条工厂流水线

把整个平台想象成一条工厂流水线：有"接单的"（API 服务）、"调度工人的"（调度器）、"干活的工人"（Runner）、"仓库"（数据库 + 文件存储）、"展示橱窗"（前端）。

```
┌──────────────┐   请求    ┌─────────────────────────────────┐
│  前端 (React) │─────────►│  API 服务 (Rust)                 │
│  轨迹查看器   │   实时流   │  ├─ 接口：批次/运行/轨迹/报告      │
│  仪表盘       │◄─────────│  ├─ 实时事件广播                  │
└──────────────┘           │  └─ 调度器：分派任务、管租约       │
                           └──────┬───────────────┬──────────┘
        ┌─────────────────────────┤               │
        ▼                         ▼               ▼
┌──────────────┐         ┌──────────────┐   ┌──────────────┐
│ PostgreSQL   │         │ 对象存储(S3)  │   │ Runner 池     │
│ 元数据/索引   │         │ 轨迹原文      │   │ (沙箱里跑 Agent)│
└──────────────┘         └──────────────┘   └──────────────┘
```

每个技术选型背后都有权衡（面试时每个都要能说出为什么）：

| 选了什么 | 为什么 | 放弃了什么 |
|---|---|---|
| Rust 后端 | 高并发实时连接 + 沙箱调度，复用 Part 5 技能 | Python 的生态便利（用适配层补）|
| PostgreSQL | 关系查询 + 半结构化都行 | 极致写入吞吐（轨迹原文不进库）|
| 轨迹存对象存储、库里只存索引 | 单条轨迹可能几十 MB，进库会拖垮一切 | 轨迹内容的 SQL 检索（用摘要字段补）|
| 实时用 SSE | 单向推送够用，复用 HTTP 基建 | 浏览器端上行（本平台不需要）|
| Runner 主动来领任务 | worker 崩了任务自动回到队列、扩容就是加 worker | 精细调度控制 |

## 49.3 数据怎么组织

平台的核心数据对象（领域模型），理清它们的关系：

```
评测集（一批测试用例）
  └─ 用例（一个任务 + 期望 + 预算）
批次（一次评测：用某个评测集 × 某个 Agent 配置）
  └─ 运行（一个用例跑一次 → 状态/分数/成本）
      └─ 轨迹（这次运行的完整过程记录，存 S3）
Agent 配置（被测对象：用哪个框架 + 哪个模型 + 哪个版本 + 哪个沙箱镜像）
```

数据库表结构的核心（完整版在仓库的 `migrations/`）：

```sql
CREATE TABLE runs (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    batch_id      UUID NOT NULL,
    case_id       TEXT NOT NULL,
    status        TEXT NOT NULL DEFAULT 'queued',   -- queued/running/passed/failed/error
    score         REAL,
    cost_usd      REAL,
    trace_url     TEXT,                             -- 指向 S3 上的轨迹文件
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_runs_batch ON runs(batch_id, status);   -- 按批次查时快
```

注意 Agent 配置里记了"用哪个沙箱镜像"（精确到第 47 章说的 digest）——这是为了**可比性**：只有两个批次的环境完全相同，对比才有意义。平台要在界面上强提示"这两个批次镜像不同，对比可能无意义"。

## 49.4 接口设计与"三端类型一致"

后端对外提供的接口（API 契约）：

```
POST   /api/batches              提交一个批次
GET    /api/batches/:id          查批次状态和统计
GET    /api/runs?batch=&status=  查运行列表（过滤、分页）
GET    /api/runs/:id/trace       拿某次运行的轨迹（分页）
GET    /api/runs/:id/stream      实时看某次运行（SSE）
GET    /api/reports/compare?a=&b= 双批次对比报告
```

这里有个全栈工程师必须解决的问题：**同一个"事件模型"要在 Rust 后端、TypeScript 前端、Python runner 三处保持完全一致**，否则对不上就出 bug。解法是：用一份 **JSON Schema 文件当"唯一真相"**，构建时自动生成三种语言的类型代码。改模型 = 改这一份 schema + 三端重新生成，CI 检查三端一致。这是第 41、42 章反复强调的"单一来源"思想的落地。

完整 API 接口（实现版，包含后续章节的所有端点）：

```
公开接口（/api，给前端和 CI）：
  POST  /api/profiles                创建 Agent 配置
  GET   /api/profiles                列出所有配置
  POST  /api/batches                 提交批次（支持幂等键）
  GET   /api/batches                 列出批次（含通过率/成本汇总）
  GET   /api/batches/:id             批次详情
  POST  /api/batches/:id/cancel      取消批次
  GET   /api/runs?batch=&status=     查运行列表（过滤、分页）
  GET   /api/runs/:id                单次运行详情
  GET   /api/runs/:id/trace          轨迹分页读取
  GET   /api/runs/:id/stream         SSE 实时流
  GET   /api/reports/compare?a=&b=   双批次对比报告
  GET   /api/stats/dashboard         仪表盘摘要统计
  GET   /api/stats/trend?days=&group_by=  通过率趋势（折线图数据源）

内部接口（/internal，仅 runner 调用，Bearer token 鉴权）：
  POST  /internal/lease              领取任务
  POST  /internal/runs/:id/heartbeat 心跳续租
  POST  /internal/runs/:id/events    批量上报轨迹事件（JSONL 格式）
  POST  /internal/runs/:id/complete  标记运行完成

系统接口：
  GET   /healthz/live                存活探针
  GET   /healthz/ready               就绪探针（检查 DB + 对象存储）
  GET   /metrics                     Prometheus 文本格式指标
```

## 49.5 仓库长什么样

```
agent-eval-platform/
├── docker-compose.yaml     # 本地一键起全栈（第 45 章的 Compose）
├── schemas/                # 事件模型的"唯一真相"（JSON Schema）
├── server/                 # Rust：API + 实时广播 + 调度器（第 50 章）
│   ├── migrations/         #   数据库表结构
│   └── src/
├── runner/                 # Python：领任务、起沙箱、跑 Agent、上报（第 50 章）
├── web/                    # React：轨迹查看器 + 仪表盘（第 51 章）
└── deploy/                 # K8s 配置 + CI/CD（第 52 章）
```

这个结构本身就是这套技能的缩影：Rust 后端、Python runner、React 前端、容器化部署——你前面九部分学的全用上了。

## 49.6 小结

- 做平台先当产品经理：想清楚谁用、解决什么、**不做什么**。
- 架构像工厂流水线：前端 + API 服务 + 调度器 + Runner 池 + 数据库/对象存储。
- 三个贯穿全局的设计原则：**轨迹不进数据库（太大，存 S3 只留索引）、评分不进沙箱（防作弊）、环境指纹不能少（保证可比和可复现）**。
- 事件模型用一份 schema 当唯一真相，生成三端类型。

下一章开始逐块实现。建议先去 `agent-eval-platform/` 跑一下 `docker compose up`，对照本章架构图把整体跑起来看一遍。

> **下一章**：后端实现——调度器怎么分派任务、轨迹怎么流式存进 S3、实时事件怎么广播。
