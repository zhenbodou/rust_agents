# 第 45 章 Docker：把应用装进"集装箱"

> "在我电脑上明明是好的，一上线就崩"——这是所有开发者的噩梦。Docker 就是来终结它的。本章从零开始：先动手用起来，再搞懂"容器到底是什么"，最后写出生产级的镜像。即使你没接触过容器也没关系，我们用类比一步步来。

## 45.1 Docker 解决什么问题

先理解痛点。你在 Mac 上写好的程序，拷到 Ubuntu 服务器上，因为系统库版本不同就跑不起来；新同事入职装环境装一整天；两个项目要不同版本的 Python，互相打架。

Docker 的思路，用**集装箱**类比最贴切：以前运货，每种货物形状不同、装卸方式不同，效率极低；自从有了标准集装箱，不管里面装什么，所有港口、货轮、卡车都能用同样的方式装卸。Docker 就是软件界的集装箱——**把你的程序和它需要的一切（系统库、依赖、配置）打包成一个标准"镜像"，任何装了 Docker 的机器都能原样运行它**。一次打包，到处运行。

```bash
# 安装：Mac/Windows 装 Docker Desktop（官网下载）；Linux 一行命令：
curl -fsSL https://get.docker.com | sh

docker run hello-world      # 验证：它会自动下载一个镜像并运行，打印一句问候
```

先把第一批命令敲熟（你第 44 章已经用过第一条）：

```bash
docker run -it --rm ubuntu:24.04 bash    # 进入一个一次性的 Ubuntu 环境
#  -it    给我一个可交互的终端
#  --rm   退出后自动删掉这个容器
# 在里面随便折腾（apt install 啥的），exit 退出后宿主机毫发无损

docker run -d --name web -p 8080:80 nginx    # 后台跑一个 nginx 网页服务器
#  -d            后台运行
#  --name web    给它起个名
#  -p 8080:80    端口映射：访问宿主机的 8080 就是访问容器里的 80
curl localhost:8080                          # 访问成功！

docker ps                  # 看正在运行的容器
docker logs -f web         # 看某个容器的日志
docker exec -it web bash   # 钻进正在运行的容器里看看（排障常用）
docker stop web            # 停止
docker rm web              # 删除
```

## 45.2 两个核心概念：镜像和容器

新手最容易搞混"镜像"和"容器"。用**模具和饼干**类比就清楚了：

- **镜像（image）= 饼干模具**：一个只读的模板，规定了"做出来长什么样"。
- **容器（container）= 用模具做出来的饼干**：镜像运行起来的实例。

一个模具能做出很多饼干，一个镜像也能同时跑出很多互不干扰的容器：

```bash
docker run -d --name web1 -p 8081:80 nginx
docker run -d --name web2 -p 8082:80 nginx   # 同一个镜像，两个独立容器
```

**重要：容器里写的数据是临时的**——容器一删，里面的数据就没了。要长久保存数据（比如数据库），得"挂载一个卷"把数据存到容器外面：

```bash
# -v pgdata:... 把数据存到一个叫 pgdata 的持久卷里，容器删了数据还在
docker run -d --name db -v pgdata:/var/lib/postgresql/data \
  -e POSTGRES_PASSWORD=dev postgres:17     # -e 设置环境变量
```

### 写一个 Dockerfile：把自己的程序打成镜像

`Dockerfile` 是制作镜像的"配方"，每一行是一步：

```dockerfile
FROM python:3.12-slim     # 基础镜像：站在一个已经装好 Python 的镜像肩膀上
WORKDIR /app              # 设定工作目录
COPY main.py .            # 把你的代码拷进镜像
CMD ["python", "main.py"] # 容器启动时默认运行这个命令
```

```bash
docker build -t my-app:v1 .    # 按配方构建镜像，命名为 my-app:v1
docker run --rm my-app:v1      # 运行你自己的镜像！
```

## 45.3 容器到底是什么（面试爱问）

这是面试杀手题。答案很反直觉：**容器不是虚拟机，它就是宿主机上一个普通进程，只不过被三种机制"框"住了**：

1. **命名空间（Namespaces）**：让这个进程"以为"自己独占整个系统——看不见别的进程、有自己的网络和文件系统。其实是一种"障眼法"。
2. **控制组（Cgroups）**：限制它最多能用多少 CPU、多少内存。
3. **联合文件系统**：镜像由很多"只读层"叠起来，容器在最上面加一个"可写层"——这样很多容器能共享底层、省空间。

由此推出一个重要结论：**容器和宿主机共享同一个操作系统内核**（这就是它比虚拟机轻、秒级启动的原因）。但代价是：内核要是有漏洞，容器里的程序就可能"逃逸"出来攻击宿主机。**这正是第 47 章要用 gVisor 这类"强隔离"方案的原因**——跑不可信代码（比如 Agent 生成的代码）时，普通容器的隔离不够强。这条因果链能一口气讲清楚，面试官会高看你。

## 45.4 写出高效的镜像：理解"层"和缓存

Dockerfile 里每条会改文件的指令（COPY、RUN）都会生成一"层"。Docker 构建时会缓存这些层，但有个规则：**某一层变了，它之后的所有层都得重建**。

这带来一个重要的排序原则：**把"不常变的"放前面，"常变的"放后面**。否则你改一行代码，整个依赖都要重装，构建慢得要命：

```dockerfile
# ✗ 错误：改一行代码 → 依赖全部重装，每次构建 10 分钟
COPY . /app
RUN pip install -r /app/requirements.txt

# ✓ 正确：先拷依赖清单装依赖（不常变），最后才拷源码（常变）
COPY requirements.txt /app/
RUN pip install -r /app/requirements.txt
COPY src/ /app/src/          # 改代码只重建这一步，秒级
```

## 45.5 多阶段构建：把镜像做小做安全

一个关键技巧叫**多阶段构建**：用一个"大"镜像编译程序，再把编译产物拷进一个"小"镜像来运行。这样最终镜像里只有运行必需的东西，又小又安全。看评测平台后端（Rust）的例子：

```dockerfile
# 第一阶段：用完整的 Rust 镜像来编译（这个镜像很大，1.5GB）
FROM rust:1.85-slim AS builder
WORKDIR /app
COPY . .
RUN cargo build --release --bin eval-server

# 第二阶段：换一个极小的镜像，只把编译好的程序拷过来运行
FROM gcr.io/distroless/cc-debian12:nonroot AS runtime
COPY --from=builder /app/target/release/eval-server /usr/local/bin/
USER nonroot                 # 用非 root 用户运行（安全！第 44 章讲过）
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/eval-server"]
```

最终镜像只有约 30MB（直接用 rust 镜像打包是 1.5GB+），而且那个 `distroless` 基础镜像里**连 shell 都没有**——攻击者就算进来了也无从下手。

前端（Trace Viewer）同理：用 Node 镜像构建出静态文件，再用 nginx 镜像托管。nginx 配置里有 Agent 平台特有的两行（呼应第 37 章的流式）：

```nginx
location /api/ {
    proxy_pass http://backend:8080;
    proxy_buffering off;        # SSE 流式必须关掉缓冲，否则实时性没了
    proxy_read_timeout 1h;      # 长连接别被默认 60 秒掐断
}
location / { try_files $uri /index.html; }   # 单页应用的路由兜底
```

## 45.6 镜像安全要点

镜像安全是 Agent 平台的重点（要跑不可信代码）。几条核心实践：

| 实践 | 怎么做 |
|---|---|
| 用非 root 运行 | Dockerfile 写 `USER` + K8s 再加 `runAsNonRoot` 双保险 |
| 扫漏洞 | CI 里用 `trivy image` 扫，发现高危就阻断发布 |
| 不把密钥打进镜像 | 密钥在运行时注入，绝不写进 Dockerfile |
| 最小化 | 用 distroless / alpine，运行镜像里不装 curl、bash |

还有个容易忘但很重要的 `.dockerignore` 文件，防止把敏感文件打进镜像：

```
.git
target/
node_modules/
.env*
```

## 45.7 Docker Compose：一键起一整套服务

真实应用往往是好几个服务（后端 + 数据库 + 前端）一起跑。一个个 `docker run` 太麻烦，用 **Compose** 把它们写进一个文件，一条命令全起来：

```yaml
# docker-compose.yaml
services:
  db:
    image: postgres:17-alpine
    environment:
      POSTGRES_PASSWORD: dev
    volumes: ["pgdata:/var/lib/postgresql/data"]
    healthcheck:                                    # 健康检查
      test: ["CMD-SHELL", "pg_isready -U postgres"]
      interval: 2s
      retries: 15

  backend:
    build: { context: ./server }
    environment:
      DATABASE_URL: postgres://postgres:dev@db:5432/evalplatform
      ANTHROPIC_API_KEY: ${ANTHROPIC_API_KEY}       # 从你的环境透传，不写死
    ports: ["8080:8080"]
    depends_on:
      db: { condition: service_healthy }            # 等数据库真正可用再启动

  web:
    build: { context: ./web }
    ports: ["5173:8080"]
    depends_on: [backend]

volumes:
  pgdata:
```

```bash
docker compose up        # 一条命令，整套服务全起来
```

那个 `condition: service_healthy` 很关键——它解决了"后端比数据库先起来、连不上数据库就崩"的经典问题。

## 45.8 排查容器问题

```bash
docker logs -f --tail 100 backend         # 看日志
docker exec -it backend sh                # 进容器内部看看
docker stats                              # 实时看各容器吃多少 CPU/内存
docker inspect backend | jq '.[0].State'  # 看容器状态（是不是被 OOM 杀了？）
```

两个高频问题：**容器退出码 137** 几乎总是"内存超限被杀（OOMKilled）"——要么调高内存限制，要么查内存泄漏；**distroless 容器进不去**（没有 shell），得用一个临时调试容器附加上去（第 46 章 K8s 的 `kubectl debug` 会细讲）。

## 45.9 小结与练习

- 容器是软件界的集装箱：打包一次、到处运行；镜像是模具、容器是饼干。
- 容器本质是被命名空间 + cgroups 框住的普通进程，共享宿主内核——这是隔离的上限，也是第 47 章要强隔离的原因。
- Dockerfile 按"变化频率"排层（不常变的在前）；多阶段构建把镜像做小做安全；运行一律非 root + 最小镜像。
- Compose 用 healthcheck 管依赖顺序；退出码 137 = 内存超限。

**练习**

1. 把 mini-claude-code 打成一个 distroless 镜像，目标小于 40MB，用 trivy 扫描做到 0 个高危漏洞。
2. 给第 39 章你写的最小后端写一个 Dockerfile，用多阶段构建，最终镜像不含构建工具。
3. 写出评测平台的三个镜像 + compose 文件，做到 `docker compose up` 一键起全栈（数据库 + 后端 + 前端）。

> **下一章**：Kubernetes——当你有几百个容器要管、要自动扩缩容、要自愈时，就需要它。我们从"它解决什么问题"讲起，把评测平台真正跑在 K8s 上。
