# 第 40 章 Python 从零开始

> Python 是 AI 圈的"普通话"——算法团队的 RL 训练、各种 Agent 框架、评测脚本，几乎全用 Python 写。它也是最适合零基础入门的语言之一：语法干净、读起来像英语。本章假设你**没写过 Python**。如果你跟着 Part 7 学过 JavaScript，会有不少地方似曾相识，我会在旁边点一下；没学过也完全不影响，我们从装环境开始。

## 40.1 装好环境，敲下第一行

Python 的环境管理以前很乱，现在有了 `uv` 这个神器（又快又省心，已是 2026 年的事实标准），一个工具全搞定：

```bash
# 安装 uv（它连 Python 本体都帮你装）
curl -LsSf https://astral.sh/uv/install.sh | sh
uv python install 3.12

# 进入交互式解释器（叫 REPL，类似浏览器的 Console，敲一行立刻出结果）
uv run python
>>> print("你好 Python")
>>> 1 + 1
>>> exit()
```

Python 和 JS 一样是"解释执行"的（写完直接跑，不用像 Rust 那样先编译）。它有个一眼就能看出的特色：**用缩进表示代码的层次，没有大括号**。同一层的代码要对齐，约定用 4 个空格。缩进错了会直接报错——这强制你写出整齐的代码。

## 40.2 基础语法

### 40.2.1 变量和基本类型

```python
name = "mini-claude-code"     # 直接写"名字 = 值"就是声明，不用 let/const
count = 0
cost = 0.0312                 # 整数和小数是不同类型
ok = True                     # 布尔值首字母大写！True / False（注意和 JS 的 true 不同）
nothing = None                # 表示"空"，相当于 JS 的 null

# f-string：往字符串里塞变量（相当于 JS 的反引号模板）
print(f"工具 {name} 成本 ${cost:.4f}")   # :.4f 表示保留 4 位小数

# 常用字符串操作
"  trace.log ".strip()        # 去掉两边空格
"cargo test".split(" ")       # 切成 ["cargo", "test"]
"test" in "cargo test"        # True —— in 判断"包含"
len("agent")                  # 5 —— 求长度用 len(...) 包起来
```

顺带说个习惯：Python 里变量名用下划线 `snake_case`（如 `tool_name`），而 JS 用驼峰 `camelCase`（如 `toolName`）。

### 40.2.2 装数据的四种容器

```python
# 列表 list：一串有顺序的东西（相当于 JS 的数组）
events = ["tool_call", "tool_result"]
events.append("done")         # 末尾追加
events[0]                     # 第一个
events[-1]                    # 倒数第一个（Python 支持负数下标，很方便）
events[1:3]                   # "切片"：取第 1 到 2 个（不含 3）——Python 招牌语法

# 字典 dict：带标签的一组数据（相当于 JS 的对象）
event = {"type": "tool_call", "tool": "bash", "ms": 320}
event["tool"]                 # 取值只能用方括号（没有 event.tool 这种点号写法！）
event.get("missing", 0)       # 取不到时给个默认值 0
"tool" in event               # 判断键在不在

# 元组 tuple：不可改的列表，常用于"一组固定的值"
point = (3, 5)
x, y = point                  # 拆开赋值

# 集合 set：自动去重
tools = {"bash", "read", "bash"}   # 结果是 {"bash", "read"}
```

**推导式**是 Python 最有特色、最常用的语法，一行完成"遍历 + 筛选 + 变形"：

```python
durations = [e["ms"] for e in events]              # 取出每个的 ms（相当于 JS 的 map）
failed    = [e for e in events if not e["ok"]]     # 只留失败的（相当于 filter）
total     = sum(e["ms"] for e in events)           # 求和

# 对照：JS 写 events.filter(e => !e.ok).map(e => e.tool)
#       Python 写 [e["tool"] for e in events if not e["ok"]]  —— 一行搞定
```

### 40.2.3 控制流和函数

```python
# 判断（注意是 elif 不是 else if，注意冒号和缩进）
if status == "passed":
    print("✓")
elif status == "failed":
    print("✗")
else:
    print("?")

icon = "✓" if ok else "✗"     # 三元表达式（值在前、条件在中，和 JS 顺序相反）

# 循环（Python 只有一种 for，就是"遍历"）
for e in events:
    print(e)
for i, e in enumerate(events):  # 同时要序号时
    print(i, e)

# 函数：def 开头
def total_ms(events, only_ok=False):     # only_ok=False 是默认参数
    """计算总耗时。"""                    # 这行三引号是函数说明（文档）
    return sum(e["ms"] for e in events if e["ok"] or not only_ok)

total_ms(events, only_ok=True)           # 调用时可以写参数名，可读性好
```

**一个 Python 头号大坑**（面试必问，现在就记住）：函数的默认参数不要用列表/字典这种"可变"的值：

```python
def add(e, log=[]):           # ✗ 这个 [] 只在定义时创建一次，所有调用共享它！
    log.append(e)
    return log
add(1)    # [1]
add(2)    # [1, 2] —— 第二次调用居然带着上次的数据！

def add(e, log=None):         # ✓ 正确写法：用 None 当默认值，进函数再创建
    if log is None:
        log = []
    log.append(e)
    return log
```

## 40.3 错误处理和读写文件

Python 用"异常"来处理错误（出错就"抛出"一个异常，你可以"接住"它）：

```python
import json

try:
    with open("trace.jsonl") as f:        # with：用完自动关文件，不用手动 close
        events = [json.loads(line) for line in f]   # 文件可以直接逐行遍历
except FileNotFoundError:                  # 文件不存在
    events = []
except json.JSONDecodeError as e:          # JSON 格式坏了
    print(f"第 {e.lineno} 行损坏")
    raise                                  # 处理不了就重新抛出，别假装没事
finally:
    print("无论成败都会执行这里")
```

那个 `with` 很重要——它保证"用完一定释放资源"（关文件、断连接等），是 Python 的好习惯。

处理 JSON 和路径的常用工具：

```python
import json
s = json.dumps(event, ensure_ascii=False)   # 对象 → 字符串
e = json.loads(s)                            # 字符串 → 对象

from pathlib import Path
p = Path("sessions") / "run-42.jsonl"        # 用 / 拼路径（跨系统通用）
p.exists()                                   # 文件在不在
list(Path("sessions").glob("*.jsonl"))       # 找出所有 .jsonl 文件
```

## 40.4 用类来组织数据

当你有"一类东西"要反复创建时，用类（class）。Python 有个特别省事的写法叫 `dataclass`，自动帮你生成构造和打印代码：

```python
from dataclasses import dataclass, field

@dataclass                          # 加这一行，下面就成了"数据类"
class ToolCall:
    tool: str                       # 字段名: 类型
    args: dict
    duration_ms: int = 0            # 带默认值
    tags: list[str] = field(default_factory=list)   # 列表默认值要用这种写法（还记得 40.2.3 的坑吗）

call = ToolCall(tool="bash", args={"cmd": "ls"})
print(call)        # 自动打印成 ToolCall(tool='bash', args={...}, ...) 很友好
```

需要带行为（方法）的类这样写：

```python
class TraceParser:
    def __init__(self, on_event):       # __init__ 是构造函数
        self.on_event = on_event        # self 相当于其他语言的 this（但要显式写出来）
        self._buffer = ""               # 下划线开头表示"内部使用，别从外面动"（君子约定）

    def feed(self, chunk: str):         # 方法的第一个参数永远是 self
        self._buffer += chunk
        lines = self._buffer.split("\n")
        self._buffer = lines.pop()      # 最后一段可能不完整，留着
        for line in lines:
            if line.strip():
                self.on_event(json.loads(line))
```

## 40.5 模块和项目结构

代码多了要拆成多个文件。Python 里一个 `.py` 文件就是一个"模块"，用 `import` 互相引用：

```python
import json                          # 导入整个模块
from pathlib import Path             # 从模块里导入某个东西
from collections import Counter      # 一个超好用的计数器
```

用 uv 管理项目的标准结构（和你前端的 package.json 是一个意思）：

```
trace-tools/
├── pyproject.toml          # 项目配置（相当于 package.json）
├── uv.lock                 # 锁定依赖版本（提交进 git）
└── src/trace_tools/
    ├── parser.py
    └── cli.py
```

```bash
uv add httpx              # 装一个第三方库（相当于 pnpm add）
uv add --dev pytest       # 装开发时才用的库（如测试工具）
uv run python -m trace_tools.cli   # 在项目环境里运行
```

每个 Python 文件末尾常有这么一段固定写法，意思是"只有当这个文件被直接运行时才执行 main"：

```python
def main():
    ...

if __name__ == "__main__":
    main()
```

## 40.6 生成器：处理大文件不爆内存

如果一个轨迹文件有几个 G，全读进内存会爆。Python 的"生成器"能逐条产出、用完即弃，内存里永远只有一条：

```python
def read_events(path):
    with open(path) as f:
        for line in f:                # 逐行读
            if line.strip():
                yield json.loads(line)    # yield 表示"产出一条，然后暂停等下次要"

# 用起来和普通循环一样，但它是"边读边给"，不会一次性占满内存
for e in read_events("huge.jsonl"):
    print(e)
```

## 40.7 综合实战：轨迹统计小工具

把本章学的全部串成一个真工具——读 mini-claude-code 的 session 文件，统计每个工具的调用次数、平均耗时、失败率：

```python
# cli.py
import json, sys
from collections import defaultdict
from dataclasses import dataclass, field
from pathlib import Path


@dataclass
class ToolStats:
    count: int = 0
    total_ms: int = 0
    failures: int = 0

    @property                          # @property 让你能像访问字段一样调用它：s.avg_ms
    def avg_ms(self):
        return self.total_ms / self.count if self.count else 0


def read_events(path):
    """逐行读取，单行损坏只跳过、不毁掉整个分析。"""
    with open(path) as f:
        for lineno, line in enumerate(f, 1):
            if not line.strip():
                continue
            try:
                yield json.loads(line)
            except json.JSONDecodeError:
                print(f"警告: {path}:{lineno} 行损坏，跳过", file=sys.stderr)


def analyze(paths):
    stats = defaultdict(ToolStats)     # defaultdict：访问不存在的键时自动建一个，免去判断
    for path in paths:
        for e in read_events(path):
            if e.get("type") != "tool_result":
                continue
            s = stats[e.get("tool_name", "?")]
            s.count += 1
            s.total_ms += e.get("duration_ms", 0)
            if e.get("is_error"):
                s.failures += 1
    return stats


def main():
    paths = [Path(p) for p in sys.argv[1:]] or list(Path(".").glob("*.jsonl"))
    if not paths:
        sys.exit("用法: cli.py <session.jsonl>...")

    stats = analyze(paths)
    print(f"{'工具':<12}{'次数':>6}{'均耗时':>10}{'失败率':>8}")
    for tool, s in sorted(stats.items(), key=lambda kv: -kv[1].count):
        print(f"{tool:<12}{s.count:>6}{s.avg_ms:>9.1f}ms{s.failures / s.count:>7.1%}")


if __name__ == "__main__":
    main()
```

```bash
uv run python cli.py ~/.mcc/sessions/*.jsonl
```

麻雀虽小五脏俱全：dataclass + property、生成器逐行读、defaultdict 分组、异常隔离（一行坏不影响整体）、f-string 排版——全是真实生产脚本的写法。

## 40.8 小结与练习

- 缩进即语法；变量直接赋值；`snake_case` 命名；f-string 拼字符串。
- 四种容器：list、dict（只能方括号取值）、tuple、set；推导式一行搞定 map/filter。
- 头号坑：默认参数别用可变值（用 None 哨兵）；异常处理 + `with` 自动释放资源。
- dataclass 是建模数据的默认姿势；生成器（yield）惰性处理大文件不爆内存；uv 管项目。

**练习**

1. 完成 40.7，并加一个 `--json` 参数让它输出 JSON 格式（给前端当数据源用）。
2. 用推导式一行完成：提取所有失败的命令、按工具分组成字典、耗时前 5。先用 JS 写再翻成 Python，体会两种风格。
3. 写一个 `Session` 类封装一个 JSONL 文件，让 `len(session)`、`for e in session`、`session[3]` 都能用（提示：实现 `__len__`、`__iter__`、`__getitem__` 这几个特殊方法）。
4. 用 `subprocess` 写一个最小"bash 工具"函数：执行命令、限时 30 秒、捕获输出，超时和出错都抛自定义异常——这就是 Agent 工具的 Python 版内核（对照第 22 章 Rust 版）。

> **下一章**：Python 进阶——类型标注、异步并发（asyncio）、用 pydantic 做数据校验，这些是写 Agent 后端和 rollout 基础设施的必备装备。
