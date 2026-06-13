# 第 44 章 Linux 与命令行：从零开始

> Docker 容器里跑的是 Linux，K8s 节点是 Linux，沙箱是 Linux，生产服务器也是 Linux。整个第九部分都站在 Linux 上，所以先把这块地基打牢。本章假设你只会最基本的 `cd`/`ls`，目标是让你能在一台陌生服务器上自如地查问题。别担心命令记不住——常用的就十几个，用几次就熟了。

## 44.1 先搞个练习环境

你需要一个 Linux 环境来练。三选一，推荐第一种（顺便预习下一章的 Docker）：

```bash
# ① 一行命令进入一个干净的 Ubuntu（装完 Docker 后，下一章细讲）
docker run -it --rm ubuntu:24.04 bash

# ② Mac 自带类 Unix 环境，大部分命令通用
# ③ Windows 装 WSL2：运行 wsl --install，就有了真正的 Ubuntu
```

## 44.2 文件系统：一栋从 / 出发的大楼

Windows 有 C 盘 D 盘，Linux 不一样——它把所有东西组织成**一棵从 `/`（叫"根"）出发的树**，像一栋大楼，`/` 是大门，里面一层层的文件夹是各个房间：

```
/
├── home/zhangjie/    # 你的家目录（用 ~ 这个符号代表它）
├── etc/              # 各种配置文件放这
├── var/log/          # 日志放这（查问题第一现场）
├── tmp/              # 临时文件
└── usr/bin/          # 大部分命令程序本体在这
```

最常用的导航和文件操作命令，在练习环境里挨个敲一遍：

```bash
pwd                  # 我现在在哪个文件夹
ls -la               # 列出文件：-l 显示详情，-a 连隐藏文件（. 开头的）也显示
cd /var/log          # 进入某个文件夹（/ 开头是"从大门算起"的绝对路径）
cd ..                # 回上一层
cd                   # 回家目录

mkdir -p a/b/c       # 建文件夹（-p 顺便把中间的也建了）
cp 源 目标            # 复制（复制文件夹要加 -r）
mv 旧 新              # 移动或改名（同一个命令）
rm 文件              # 删除（⚠️ 没有回收站，删了就没了）
rm -rf 文件夹/        # 强制递归删除 ——⚠️ 敲回车前把路径默念三遍

cat file.txt         # 把整个文件打印出来（小文件用）
less huge.log        # 分页看大文件（按 q 退出、/ 搜索）——大文件别用 cat
tail -f app.log      # 实时盯着一个文件的新增内容（看实时日志的标准姿势）
```

## 44.3 命令长什么样、怎么求助

命令的格式都差不多：`命令 -选项 参数`。记不住选项？两个求助方式：

```bash
ls --help            # 快速看用法
man ls               # 看完整手册（按 q 退出）
```

还有个重要概念：**环境变量**——给程序的"键值对配置"。你第 4 章设的 `ANTHROPIC_API_KEY` 就是它：

```bash
echo $HOME           # 读一个变量的值（前面加 $）
export API_KEY=sk-xxx           # 设一个变量（当前窗口及它开的程序有效）
API_KEY=sk-xxx cargo run        # 只给这一条命令临时设
# 想永久生效，写进 ~/.bashrc 文件（每次开终端都会自动执行它）
```

其中有个特殊变量 `PATH`，它是一串文件夹路径。你输入 `ls` 时，系统就是按 `PATH` 里的文件夹挨个找叫 `ls` 的程序——这就是为什么有时装了软件却"找不到命令"，因为它不在 PATH 里。

## 44.4 管道：把命令像流水线一样串起来

这是命令行最强大的地方，也是 Unix 的精髓。每个命令都有"输入"和"输出"。**管道符 `|` 能把上一个命令的输出，直接喂给下一个命令的输入**——像工厂流水线，一道工序接一道工序。

```bash
# 重定向：把输出存进文件（而不是打印到屏幕）
cargo build > build.log          # 输出写进文件（覆盖）
cargo build > all.log 2>&1       # 把正常输出和错误输出都收进去（CI 脚本天天用，记住它）

# 管道：一个命令的输出 → 下一个命令的输入
ls -la | wc -l                              # 列出文件，再数有几行 = 文件数
cat trace.jsonl | grep tool_call | wc -l    # 读文件 → 筛出含 tool_call 的行 → 数行数
```

处理日志、轨迹这类文本，有"四件套"工具，是运维的瑞士军刀。不用全记，知道各自干嘛、用到再查：

```bash
# grep：筛选出含某个词的行
grep -rn "error" src/            # 在 src 文件夹里递归找 error，带行号

# awk：按"列"处理（默认按空格分列，$1 是第一列）
awk '{print $1}' access.log      # 只打印每行的第一列
awk '{s+=$2} END {print s}' x    # 把第二列加起来求和

# sed：批量替换文本
sed -i 's/8080/9090/' config.yaml   # 把文件里的 8080 全换成 9090

# jq：专门处理 JSON（Agent 工程师用得比 awk 还多）
cat trace.jsonl | jq -r 'select(.type=="tool_call") | .tool_name' | sort | uniq -c
# ↑ 读轨迹 → 筛出 tool_call → 取工具名 → 排序 → 统计每个出现几次
```

这套管道的威力：第 40 章用 50 行 Python 写的轨迹统计，临时查一下一条管道就出来了。**写成工具用 Python，临时分析用管道**——两手都要会。

## 44.5 权限：谁能读、谁能写、谁能运行

Linux 是多用户系统，每个文件都规定了"谁能拿它怎么样"。`ls -l` 开头那串字符就是权限：

```
-rwxr-xr--   表示这个文件：
 │└┬┘└┬┘└┬┘
 │ │  │  └─ 其他人：r--（只能读）
 │ │  └──── 同组的人：r-x（读 + 执行）
 │ └─────── 拥有者：rwx（读 + 写 + 执行）
 └───────── 类型：- 是文件，d 是文件夹
```

常用操作：

```bash
chmod +x deploy.sh        # 给脚本加"可执行"权限（下载的脚本要跑前先这么做）
chmod 600 ~/.ssh/id_key   # 设成只有自己能读写（私钥的标准权限）
sudo 命令                  # 以管理员（root）身份执行（装软件、重启服务时用）
```

**一条安全铁律**：别用 root（管理员）身份跑你的服务。因为 root 进程一旦被攻破，整台机器就沦陷了。下一章 Dockerfile 里的 `USER`、第 46 章 K8s 的 `runAsNonRoot`，都是在强制这条规矩。

## 44.6 进程与信号：管理正在运行的程序

每个正在运行的程序是一个"进程"。查看和控制它们：

```bash
ps aux | grep eval-server    # 找到某个程序的进程（拿到它的编号 PID）
top                          # 实时看哪个进程吃 CPU/内存（按 q 退出）

# "信号"是给进程"发通知"的方式
kill PID                     # 发 SIGTERM：礼貌地请它退出（它有机会先清理）
kill -9 PID                  # 发 SIGKILL：立刻强制杀死（不给清理机会，最后手段）

cargo run &                  # 命令后加 & = 放到后台运行
Ctrl + C                     # 杀掉当前正在前台运行的程序
```

**为什么 SIGTERM 值得记**：第 46 章 K8s 停止一个服务的流程就是"先发 SIGTERM 等几秒、还不退再发 SIGKILL"。所以你的服务收到 SIGTERM 后，应该先把手头的请求处理完再退出（这叫"优雅关闭"，第 50 章会写这个处理逻辑）。

## 44.7 网络与远程：查端口、连服务器

```bash
# "8080 端口被谁占了" —— 日常最高频问题
lsof -i :8080               # 看哪个进程占着 8080

curl localhost:8080/healthz # 探测一个 HTTP 接口（第 39 章学过）
ping example.com            # 测能不能连通

# SSH：登录远程服务器（运维的大门）
ssh user@server.com         # 登录上去
scp 本地文件 user@host:/tmp/  # 拷文件过去
```

## 44.8 磁盘与软件安装

```bash
df -h                       # 各磁盘用了多少（"磁盘满了"第一个查这个）
du -sh */ | sort -rh | head # 当前文件夹下谁最占空间（第二个查这个）

tar -czf logs.tar.gz logs/  # 打包压缩成一个文件
tar -xzf logs.tar.gz        # 解压

sudo apt update && sudo apt install -y ripgrep jq    # 装软件（Ubuntu）
```

## 44.9 Shell 脚本：把一串操作固化下来

把几条常用命令写进一个 `.sh` 文件，就成了脚本，以后一键运行。看一个"服务体检"脚本：

```bash
#!/usr/bin/env bash
set -euo pipefail        # 脚本第一行永远写它：出错就停、用了没定义的变量就报错

SERVICE="${1:-eval-server}"          # 取第一个参数，没传就默认 eval-server

# 检查进程在不在
if ! pgrep -f "$SERVICE" > /dev/null; then
    echo "✗ $SERVICE 没在运行"
    exit 1                           # 退出码非 0 表示"失败"
fi

# 检查健康接口
code=$(curl -s -o /dev/null -w "%{http_code}" localhost:8080/healthz)
if [[ "$code" == "200" ]]; then
    echo "✓ 健康检查通过"
else
    echo "✗ 健康检查失败：HTTP $code"
    exit 1
fi
```

```bash
chmod +x check.sh && ./check.sh    # 加执行权限后运行
```

记住几点就能读懂大多数脚本：变量用时加 `$` 并用引号包（`"$SERVICE"`，防止有空格出错）、`$(命令)` 表示"把命令的输出取出来"、退出码 0 表示成功非 0 表示失败。逻辑复杂（超过 50 行）就改用 Python，脚本只适合"把几条命令串起来重复用"。

## 44.10 综合演练：模拟一次真实排障

把全章串起来。场景：同事说"评测服务挂了"，你 SSH 上去查。跟着这条**"进程→端口→健康→日志→依赖→资源"**的链路走：

```bash
pgrep -f eval-server                 # 1. 进程还在吗？→ 在
lsof -i :8080                        # 2. 端口监听着吗？→ 在
curl -v localhost:8080/healthz       # 3. 服务自己说啥？→ 返回 503！
tail -100 /var/log/eval-server.log | grep -i error
                                     # 4. 日志说啥？→ "数据库连接被拒绝"
pgrep -f postgres                    # 5. 数据库进程？→ 没了！
df -h                                # 6. 为啥没了？→ 磁盘 100% 满了
du -sh /var/* | sort -rh | head -3   # 7. 谁占的？→ /var/log 占了 80G
truncate -s 0 /var/log/runaway.log   # 8. 止血：清空那个失控的日志文件
sudo systemctl start postgresql      # 9. 把数据库拉起来
curl localhost:8080/healthz          # 10. 再测 → 200 ✓ 恢复了
```

这套排查链路，和第 46 章 K8s 排障（describe → logs → events）是**完全一样的思路**，只是命令换了皮。建立这套"顺藤摸瓜"的排查直觉，比记住任何单个命令都重要。

## 44.11 小结与练习

- 一切从 `/` 出发，`~` 是家，`rm -rf` 前默念路径；大文件用 `less`/`tail -f` 别用 `cat`。
- 管道 `|` 把命令串成流水线，grep/awk/sed/jq 做临场数据分析；`> file 2>&1` 收集全部输出。
- 权限分"拥有者/同组/其他人"三组 rwx；服务别用 root 跑；SIGTERM 礼貌退、SIGKILL 强杀。
- 排障链路：进程→端口→健康→日志→依赖→资源；脚本第一行 `set -euo pipefail`。

**练习**

1. 在 Ubuntu 容器里建一个普通用户，用它跑 `python3 -m http.server`，观察它能不能占用 80 端口（不能，查查为什么 root 才能用 1024 以下的端口）。
2. 用一条管道，统计 mini-claude-code 某个 session 文件里每种工具的调用次数并排序，和你第 40 章 Python 版的输出对照。
3. 写一个 `watchdog.sh`：每 10 秒探测一次健康接口，连续失败 3 次就杀掉旧进程重新拉起，所有动作带时间戳记进日志——一个土法版的"健康守护"（第 46 章会看到 K8s 的原生版）。
4. 用 `fallocate -l 1G bigfile` 造一个大文件把小分区占满，观察服务报错，再走一遍 44.10 的排查链路。

> **下一章**：Docker——把你的应用和它的全部依赖打包成一个"集装箱"，到哪都能一键运行，彻底告别"在我电脑上是好的"。
