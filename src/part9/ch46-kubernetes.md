# 第 46 章 Kubernetes：从入门到运维 Agent 服务

> 用 Docker 跑几个容器没问题，但当你要跑几百个、要它们自动扩容、挂了自动重启、滚动更新不停服时，手动管就崩溃了。Kubernetes（简称 K8s）就是"容器的自动管家"。本章从"它解决什么问题"讲起，最后把评测平台真正跑在 K8s 上。这是运维岗位的硬技能，我们一步步来。

## 46.1 K8s 的核心思想：你说目标，它来达成

先理解 K8s 最反直觉、也最核心的一点。普通脚本是**命令式**的——你写下每一步"先干这、再干那"。K8s 是**声明式**的——你只描述"我想要的最终状态"，K8s 自己想办法达成并一直维持。

用**自动驾驶**类比：命令式像手动挡，你得自己换挡、给油、刹车；声明式像设好导航目的地，车自己开过去，遇到堵车自己绕路。你对 K8s 说"我要 3 个这个服务的副本一直运行着"，然后:

- 挂了一个？K8s 自动补一个，永远维持 3 个；
- 要升级？K8s 一个个换，全程保持有副本在服务；
- 流量大了？（配好规则后）自动加副本。

这个"不断检查实际状态、把它拉回到你期望状态"的机制叫**控制循环**——其实和你 Part 1 学的 Agent Loop（观察→对比→行动）异曲同工。理解了这点，K8s 里所有概念都是"某种期望状态的描述"。

## 46.2 上手：本地起一个真集群

不需要云账号，用 kind（Kubernetes in Docker）就能在自己电脑上起一个真集群：

```bash
brew install kind kubectl          # 两个工具：kind 起集群，kubectl 是遥控器
kind create cluster --name dev     # 一分钟后你就有了一个 K8s 集群
kubectl get nodes                  # 看到一个就绪的节点
```

`kubectl` 是你和集群对话的唯一工具，所有命令都是同一个句式：`kubectl 动词 资源类型`：

```bash
kubectl get pods            # 列出 pod（K8s 里运行容器的最小单位）
kubectl get pods -A         # 所有命名空间的（能看到系统组件）
```

先用命令式快速体验一下"自愈"：

```bash
kubectl create deployment hello --image=nginx --replicas=2   # 我要 2 个 nginx
kubectl get pods -w          # -w 持续观察，看到两个 pod 从创建到运行

# 见证自愈：删掉一个，K8s 立刻补一个新的
kubectl delete pod -l app=hello --wait=false && kubectl get pods -w
# 你声明了要 2 个，它就永远帮你维持 2 个
```

## 46.3 几个核心概念

K8s 的资源对象很多，先认识最常用的几个，都是"描述某种期望状态"：

| 对象 | 它描述什么 | 类比 |
|---|---|---|
| **Pod** | 一组共同运行的容器（最小单位） | 一个工位 |
| **Deployment** | "某个无状态服务要 N 个副本" | 排班表："这个岗位常备 3 人" |
| **Service** | 一组 pod 的稳定访问入口 | 总机号码（不管接线员换谁）|
| **Job** | "跑完就结束"的一次性任务 | 临时工干完一票就走 |
| **ConfigMap / Secret** | 配置 / 密钥 | |

生产里不用命令式，而是把期望状态写成 **YAML 文件**（声明式），提交进 git。YAML 有四个固定部分：`apiVersion`、`kind`（什么资源）、`metadata`（名字/标签）、`spec`（期望状态的具体内容）。

## 46.4 评测平台的核心配置

把评测平台后端部署上去。这份 Deployment 的每一行都有理由，我加了注释：

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: eval-backend
spec:
  replicas: 3                              # 要 3 个副本
  strategy:
    rollingUpdate: { maxUnavailable: 0, maxSurge: 1 }   # 滚动更新时不减少可用数
  selector: { matchLabels: { app: eval-backend } }
  template:
    metadata:
      labels: { app: eval-backend }
    spec:
      containers:
        - name: server
          image: registry.internal/eval-backend@sha256:abc...   # 用 digest 不用 tag！
          resources:
            requests: { cpu: 500m, memory: 512Mi }   # 至少需要这么多（调度依据）
            limits: { cpu: "2", memory: 1Gi }        # 最多用这么多（超内存会被杀）
          readinessProbe:                            # "就绪探针"：没就绪不给它导流量
            httpGet: { path: /healthz/ready, port: 8080 }
          livenessProbe:                             # "存活探针"：卡死了就重启它
            httpGet: { path: /healthz/live, port: 8080 }
          lifecycle:
            preStop: { exec: { command: ["sleep", "5"] } }  # 退出前缓冲，把在途请求处理完
```

三个最常被问的细节，理解了就懂了一半 K8s 运维：

- **就绪探针 vs 存活探针**：就绪失败 = 暂时别给它流量（但不重启，比如数据库短暂抖动）；存活失败 = 它死透了，重启。把数据库健康放进存活探针是经典错误——数据库一抖整批服务全重启，雪崩。
- **requests vs limits**：requests 是"我至少要这么多"（K8s 据此安排到哪台机器），limits 是"最多用这么多"（超内存会被强杀，就是第 45 章那个退出码 137）。
- **preStop sleep**：K8s 通知服务下线和真正停掉之间有延迟，先睡几秒把手头请求处理完再退，避免丢请求（呼应第 44 章的 SIGTERM 优雅退出）。

## 46.5 评测批次 = Job，自动扩缩 = HPA

**评测批次**天然适合用 Job（跑完即止、可设并行度、失败重试）：

```yaml
apiVersion: batch/v1
kind: Job
metadata: { name: eval-run-001 }
spec:
  parallelism: 32       # 同时 32 个 worker 一起跑
  completions: 500      # 总共 500 个用例
  backoffLimit: 3       # 失败最多重试 3 次
```

**自动扩缩容**用 HPA（Horizontal Pod Autoscaler）——根据指标自动调副本数。比如 rollout worker 根据"任务队列有多长"扩缩：

```yaml
apiVersion: autoscaling/v2
kind: HorizontalPodAutoscaler
metadata: { name: rollout-workers }
spec:
  scaleTargetRef: { kind: Deployment, name: rollout-workers }
  minReplicas: 2
  maxReplicas: 200          # 队列堆积时最多扩到 200 个
  metrics:
    - type: External
      external:
        metric: { name: queue_depth }
        target: { type: AverageValue, averageValue: "10" }   # 每个 worker 平均盯 10 个任务
```

## 46.6 配置、密钥与发布

```bash
# 创建密钥（注意：base64 不是加密，真正的安全要靠下面的进阶方案）
kubectl create secret generic eval-secrets --from-literal=ANTHROPIC_API_KEY=sk-...
```

生产发布的成熟做法（第 48 章和 48a 补充会细讲）：YAML 进 git、用 GitOps 工具（Argo CD）让"集群状态自动向 git 看齐"——改配置就是提一个 commit，回滚就是 git revert。风险大的变更走"金丝雀"：先放 5% 流量，没问题再逐步全量。

## 46.7 排障工具箱

K8s 排障和第 44 章 Linux 排障是同一套思路，命令换了皮：

```bash
kubectl get pods                      # 全局状态（看谁不正常）
kubectl describe pod eval-backend-xxx # 看详细事件（为什么调度失败/拉不到镜像/探针挂了）
kubectl logs -f deploy/eval-backend   # 看日志
kubectl logs --previous pod-xxx       # 看崩溃前那次的日志（关键！）
kubectl exec -it pod-xxx -- sh        # 进容器内部
```

高频故障速查表：

| 现象 | 第一反应 |
|---|---|
| `Pending`（一直起不来） | describe 看调度失败原因：资源不够 / 没合适节点 |
| `ImagePullBackOff` | 镜像名写错 / 拉取没权限 |
| `CrashLoopBackOff`（反复重启） | `logs --previous` 看崩溃日志，常是配置缺失 |
| `OOMKilled`（退出码 137） | 内存 limit 太低或泄漏 |

## 46.8 小结与练习

- K8s 是"容器的自动管家"：你声明期望状态，它通过控制循环不断维持（和 Agent Loop 同构）。
- 核心对象：Pod（最小单位）、Deployment（N 副本无状态服务）、Service（稳定入口）、Job（一次性任务）、HPA（自动扩缩）。
- 运维三要点：就绪 vs 存活探针、requests vs limits、preStop 优雅下线。
- 排障从 `describe` 和 `logs --previous` 开始，思路同第 44 章 Linux 排障。

**练习**

1. 用 kind 起本地集群，把评测平台的后端 + 数据库部署上去，访问验证能跑通。
2. 故意制造三种故障：内存 limit 设太小触发 OOMKilled、镜像名写错触发 ImagePullBackOff、就绪探针指向错端口——每种都用 `describe`/`logs` 定位出来，写一份排障记录。
3. 给一个 worker 配 HPA，往队列里压入大量任务，观察它从 2 个副本自动扩容的过程。

> **下一章**：Agent 容器服务——专门讲怎么为 Agent 提供安全、快速、可复现的"沙箱"环境。这是 RL 训练和评测的地基。
