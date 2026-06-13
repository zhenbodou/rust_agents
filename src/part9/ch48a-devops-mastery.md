# 第 48 章 补充 · DevOps 精通：供应链安全、密钥治理与 SRE

> 第 44–48 章建立了 Linux、Docker、K8s、Git/CI/CD 的工作能力。要达到能为 Agent 平台扛起生产运维的专家水准，还差三块在事故复盘里反复出现的硬骨头：**软件供应链安全**（你的镜像里到底有什么、是不是你构建的）、**密钥治理**（API key 泄露是 Agent 平台的头号事故源）、**SRE 实践**（SLO、可观测、容量与故障演练）。本章把这三块补齐，对应 JD 里"容器化与运维"要求的上限。

## 48a.1 软件供应链安全：从"能跑"到"可信"

Agent 平台的攻击面比普通 Web 服务大得多——它执行模型生成的代码、跑不可信仓库、持有昂贵的 LLM 凭据。供应链安全回答两个问题：**镜像里有什么漏洞？这个镜像真是我的流水线构建的吗？**

### SBOM：物料清单

SBOM（Software Bill of Materials）列出镜像里每一个组件及版本——出 CVE 时你能在几分钟内回答"哪些镜像受影响"，而不是抓瞎。

```bash
# 用 syft 生成 SBOM（SPDX/CycloneDX 标准格式）
syft registry.internal/eval-backend:v1.2.0 -o spdx-json > sbom.json
# 用 grype 扫描已知漏洞，高危即 fail CI
grype sbom:sbom.json --fail-on high
# 或直接扫镜像（trivy 一体化，CI 里最常用）
trivy image --severity HIGH,CRITICAL --exit-code 1 registry.internal/eval-backend:v1.2.0
```

把 `trivy` 接进第 48 章的 CI：构建镜像 → 扫描 → 高危漏洞阻断发布。但**别让扫描器变成狼来了**：对暂无补丁的漏洞用 `.trivyignore` 配合到期复审，区分"可利用"与"理论存在"。

### 镜像签名与来源证明

镜像扫描干净还不够——你怎么知道部署的镜像就是 CI 构建的那个，没在 registry 被掉包?用 **cosign** 签名 + **SLSA provenance** 证明来源。

```bash
# CI 里用 keyless 签名（OIDC 身份，无需管私钥，签名记录进透明日志 Rekor）
cosign sign --yes registry.internal/eval-backend@sha256:abc123...

# 部署前/准入时验签：只有 CI 身份签过的镜像才放行
cosign verify registry.internal/eval-backend@sha256:abc123... \
  --certificate-identity-regexp '^https://github.com/your-org/.+' \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com
```

在 K8s 里用**准入控制器**（Sigstore Policy Controller / Kyverno）强制"未签名镜像不准调度"——这是把信任从"约定"变成"机制"。SLSA 是分级的供应链完整性框架，核心要求一路升级：构建在隔离环境进行、生成不可篡改的 provenance（记录"用哪个 commit、哪条流水线、什么参数构建"）、消费侧验证。对 RL 沙箱镜像尤其关键——第 43 章讲过环境指纹决定训练可复现性，供应链完整性是这条链的根。

### 始终用 digest，永不用 mutable tag

第 46 章的清单里 `image: ...@sha256:abc123` 不是装腔。`:latest`/`:v1.2` 这类 tag 可被覆盖，导致"同一份 YAML 今天明天拉到不同镜像"——可复现性与安全双输。生产铁律：**部署引用 digest，tag 仅给人读**。CI 里构建后输出 digest，GitOps 仓库更新的是 digest。

## 48a.2 密钥治理：Agent 平台的头号事故源

`ANTHROPIC_API_KEY` 泄露 = 账单爆炸 + 数据风险。密钥治理有清晰的层级，从最差到最好：

| 做法 | 评级 | 问题 |
|---|---|---|
| 硬编码进代码 | 灾难 | 进 git 历史，永久泄露 |
| `.env` 文件 + gitignore | 及格 | 本地开发可以，生产不行，易误提交 |
| K8s Secret(base64) | 中 | base64 不是加密；etcd 未加密则明文 |
| 外部密钥管理（Vault/云 KMS) | 好 | 集中、可轮换、可审计 |
| 动态短期凭据 | 最佳 | 按需签发、自动过期，泄露窗口极小 |

```bash
# 防线一：提交前拦截（第 48 章 pre-commit 已提过 gitleaks)
gitleaks protect --staged    # 暂存区有疑似密钥就拒绝提交
# 防线二：扫历史（接手仓库先做一次）
gitleaks detect --source . --report-path leaks.json
```

生产的标准姿势是 **External Secrets Operator**：密钥真相在 Vault / 云 KMS,Operator 同步成 K8s Secret，应用无感知。进阶用 **Workload Identity**——pod 凭借自己的 K8s ServiceAccount 身份直接向云换取短期 token，**根本不存在长期密钥**，泄露无从谈起。

```yaml
# ExternalSecret：声明"我要 Vault 里的某条密钥",Operator 负责同步与轮换
apiVersion: external-secrets.io/v1
kind: ExternalSecret
metadata: { name: eval-secrets, namespace: eval-platform }
spec:
  refreshInterval: 1h                  # 自动轮换：Vault 改了，1 小时内集群跟上
  secretStoreRef: { name: vault-backend, kind: ClusterSecretStore }
  target: { name: eval-secrets }       # 生成同名 K8s Secret 供 envFrom 引用
  data:
    - secretKey: ANTHROPIC_API_KEY
      remoteRef: { key: llm/anthropic, property: api_key }
```

**一旦泄露的处置预案**（要提前演练，不是事后查）：立即轮换（Vault 改值→Operator 同步→滚动重启）、撤销旧 key、查审计日志评估影响范围、复盘泄露路径补防线。能多快轮换决定了损失大小——这就是为什么动态短期凭据是终局。多租户平台还要按 tenant 隔离凭据与成本配额（第 49 章），一个失控批次不该烧穿所有人的额度。

## 48a.3 SRE：让系统可运维

DevOps 把代码送上线，SRE 让它**持续可靠且可演进**。核心是用工程方法管理可靠性，而非靠人肉救火。

### SLI / SLO / Error Budget

先定义"可靠"是什么，否则无法管理。

- **SLI**（指标）：可用性 = 成功请求 / 总请求；延迟 = P99 < 500ms；对评测平台还有"批次按时完成率"。
- **SLO**（目标）：如"API 月可用性 99.9%"——即每月允许 ~43 分钟不可用。
- **Error Budget**（预算）：`100% - SLO` 就是允许出错的额度。还有预算 → 大胆发布迭代；烧光了 → 冻结发布、全力补稳定性。这把"要稳定还是要速度"的扯皮变成数据驱动的决策。

```
可用性 99.9% → 每月 43m 预算 → 本月已用 38m → 剩 5m，谨慎发布
```

### 可观测性三支柱落地

第 15 章讲过 Agent 内部的 trace。基础设施层对应三支柱：**Metrics**(Prometheus 抓取 + Grafana 看板，如 `queue_depth`、`tool_execution_duration`、`cost_usd_total`）、**Logs**（结构化 JSON + 集中检索 Loki/ELK，带 `trace_id` 串联）、**Traces**(OpenTelemetry 分布式追踪，看一个请求穿过 API→scheduler→runner→沙箱的全链路耗时）。

```yaml
# Prometheus 告警规则：症状告警（对用户的影响），而非原因告警（避免噪音淹没）
groups:
  - name: eval-platform
    rules:
      - alert: HighRunFailureRate
        expr: |
          sum(rate(eval_runs_total{status="error"}[5m]))
            / sum(rate(eval_runs_total[5m])) > 0.1
        for: 10m                       # 持续 10 分钟才告警，过滤瞬时抖动
        labels: { severity: page }
        annotations: { summary: "评测失败率 > 10%，可能是沙箱或 LLM API 异常" }
      - alert: SandboxPoolExhausted
        expr: sandbox_pool_available == 0
        for: 2m
        labels: { severity: page }
```

告警哲学：**page（电话叫醒）只留给"用户正在受影响且需人介入"的症状**；能自愈的（pod 重启、任务回队）不该 page；原因类信息进看板供排查，不进告警。告警疲劳会让真事故被淹没。

### 健康检查、优雅终止与容量

第 46 章的 readiness/liveness/preStop 是 SRE 的微观体现，这里补全闭环：**优雅终止**——应用收到 SIGTERM 后停止接新请求、排空在途请求/任务、再退出（Rust 里 tokio 监听信号，Python 里注册 signal handler)；**容量规划**——用压测（`k6`/`vegeta`）和历史指标推算需要的副本与节点，HPA 设上下限兜底（第 46 章），给 LLM 服务的内存波动留足 limits 余量；**优雅降级**——LLM API 限流时排队而非雪崩（第 17 章熔断），SSE 断线降级为轮询（第 38a 章前端侧）。

### 故障演练与无指责复盘

可靠性不是测出来的，是**练出来的**。混沌工程主动注入故障（杀 pod、断网、塞满磁盘、模拟 LLM API 503），验证系统真能自愈——第 46 章练习里的"人为制造 OOMKilled/ImagePullBackOff"就是入门。每次真实事故后做**无指责复盘**（blameless postmortem）：还原时间线、找系统性根因（不是"谁手滑"）、产出可验证的改进项并跟踪闭环。文化内核是：人会犯错，系统要能容错；追责让人隐瞒，复盘让系统进化。

## 48a.4 把一切连起来：GitOps 的完整信任链

第 46 章提了 GitOps，这里给出叠加安全后的完整形态，作为本部分的收束：

```
开发者 push
  → CI（第 48 章）:lint/test → 构建镜像 → trivy 扫描 → cosign 签名 → 输出 digest
  → 更新 GitOps 仓库（写入新 digest，而非 tag)
  → Argo CD 检测到 git 变更 → 准入控制器验签（48a.1)→ 金丝雀放量（第 46 章）
  → 指标异常自动回滚 / git revert 即回滚
  → 全程：密钥来自 External Secrets(48a.2),SLO/告警监控（48a.3)
```

每一环都把信任从"人的自觉"换成"机制的强制":git 是唯一事实来源、镜像可验来源、密钥不落地、回滚是一次 commit。这就是专家级 DevOps 与"能把服务跑起来"的本质区别——**可复现、可审计、可回滚、可观测**，四个"可"撑起 Agent 平台的生产可靠性。

## 48a.5 本章小结与练习

- 供应链：SBOM(syft)+ 扫描（trivy/grype)+ 签名（cosign keyless)+ 来源证明（SLSA），部署只认 digest，准入控制器强制验签。
- 密钥治理分级到顶是动态短期凭据；生产用 External Secrets 同步 Vault/KMS,Workload Identity 消灭长期密钥；gitleaks 双防线 + 演练好轮换预案。
- SRE:SLO/Error Budget 把稳定与速度变成数据决策；三支柱可观测；症状告警 + 优雅终止 + 故障演练 + 无指责复盘。
- GitOps 叠加安全 = 可复现、可审计、可回滚、可观测的完整信任链。

**练习**

1. 给第 48 章的 CI 流水线加供应链关卡：`trivy` 扫描高危阻断、`syft` 产出 SBOM 存档、`cosign` keyless 签名，并在 kind 集群用 Kyverno/Policy Controller 强制"未签名镜像拒绝调度"，验证篡改镜像被拦。
2. 把评测平台的密钥从 `.env` 迁移到 External Secrets（本地用 Vault dev server 模拟），演练一次"密钥泄露→轮换→滚动重启"全流程并计时，目标 5 分钟内完成轮换。
3. 给评测平台定义 3 个 SLO(API 可用性、批次完成延迟、run 失败率），用 Prometheus + Grafana 做看板与症状告警，并计算当前 error budget 消耗。
4. 做一次混沌演练：对运行中的评测平台依次注入"杀后端 pod""沙箱节点断网""LLM API 返回 503"，记录系统是否自愈、用户是否受影响，产出一份 blameless postmortem 与改进项清单。
