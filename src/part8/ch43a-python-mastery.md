# 第 43 章 补充 · Python 精通：pytest 工程、打包发布与性能

> 第 40–41 章带你从零写到了类型标注、asyncio 与 pydantic。但要在 RL 团队里维护 rollout worker、把适配层打成可分发的包、在百万级 episode 下不让 Python 成为瓶颈——还差三块硬功夫：**pytest 工程化**（fixture/参数化/mock/异步/覆盖率门禁）、**打包与分发**（构建、发布、可复现锁定）、**性能与并发**（GIL 真相、多进程、profiling）。本章把这三块补到能独立扛起 `agent-eval-platform/runner` 的水准。

## 43a.1 pytest 工程化：从能跑到能维护

第 41.6 写过一个最小测试。生产测试套件的差距在于：**fixture 管理依赖与清理、参数化消灭重复、mock 隔离外部世界、异步测试不漏 await、覆盖率进 CI 门禁**。

### Fixture：依赖注入与生命周期

fixture 是 pytest 的灵魂——它把"准备环境→注入测试→清理"做成可组合、可分作用域的依赖注入。`yield` 之前是 setup，之后是 teardown（即使测试失败也执行）。

```python
# conftest.py —— 同目录及子目录的测试自动可见，无需 import
import pytest
from pathlib import Path
from runner.adapters import MockAdapter

@pytest.fixture
def workspace(tmp_path: Path) -> Path:
    """每个测试一个干净的临时仓库（tmp_path 是 pytest 内置 fixture，自动清理）"""
    (tmp_path / "src").mkdir()
    (tmp_path / "src" / "math.py").write_text("def divide(a, b):\n    return a / b\n")
    return tmp_path

@pytest.fixture(scope="session")
def event_schema() -> dict:
    """session 作用域：整个测试会话只构建一次（重对象的标准做法）"""
    import json
    return json.loads(Path("schemas/trace-event.schema.json").read_text())

@pytest.fixture
def adapter(workspace: Path):
    """fixture 可依赖其他 fixture，形成依赖图；带清理的资源用 yield"""
    a = MockAdapter(cwd=workspace)
    yield a
    a.cleanup()        # teardown：关闭沙箱、删临时文件
```

作用域（`function`/`class`/`module`/`session`）控制重建频率：贵的资源（数据库连接、加载的 schema）用大作用域，需要隔离的状态用默认 `function`。这是测试速度与隔离性的权衡杠杆。

### 参数化：一份逻辑，多组数据

```python
@pytest.mark.parametrize(
    "raw,expected_type",
    [
        ('{"type":"tool_call","turn":1,"toolName":"bash","args":{},"callId":"c","ts":1}', "tool_call"),
        ('{"type":"run_finished","status":"success","costUsd":0.03,"ts":9}', "run_finished"),
    ],
    ids=["tool_call", "run_finished"],   # 失败时显示可读名字，而非 raw[0]/raw[1]
)
def test_parse_event(raw: str, expected_type: str):
    assert parse_event(raw).type == expected_type

@pytest.mark.parametrize("bad", ['{"type":"tool_call"}', '{"type":"unknown"}', "not json"])
def test_parse_rejects_malformed(bad: str):
    with pytest.raises((ValidationError, ValueError)):
        parse_event(bad)
```

参数化把"边界用例覆盖"从体力活变成声明式表格——加一行数据就多一个测试用例，且每个独立报告成败。

### Mock：隔离 LLM 与网络

测 Agent 代码不能真打 Anthropic API（慢、贵、不确定）。`pytest-mock` 的 `mocker` 在测试结束自动还原：

```python
def test_adapter_handles_tool_error(mocker, adapter):
    # 把 LLM 调用替换成确定性假响应（先要工具，再收到错误，再收尾）
    fake = mocker.patch.object(adapter, "_call_llm", side_effect=[
        FakeResponse(stop_reason="tool_use", tool_calls=[ToolCall("bash", {"cmd": "pytest"})]),
        FakeResponse(stop_reason="end_turn", text="测试失败，我来修"),
    ])
    result = adapter.run_episode(task="修复 divide")
    assert fake.call_count == 2
    assert result.turns == 2

# mock 异步函数用 AsyncMock
def test_gateway_step(mocker):
    sandbox = mocker.patch("runner.sandbox.Sandbox.execute",
                           new_callable=mocker.AsyncMock,
                           return_value="exit 0")
    ...
```

专家纪律：**mock 你拥有的边界（自己的 `_call_llm`/`Sandbox`），不要 mock 标准库或第三方内部**。mock 太深 = 测试和实现耦合，重构必碎。理想是只在"进程边界/网络边界"打桩。

### 异步测试

```python
# pyproject.toml: [tool.pytest.ini_options] asyncio_mode = "auto"
import pytest

async def test_rollout_step_timeout(gateway):
    """验证工具超时不会挂死训练循环（第 43 章铁律）"""
    with pytest.raises(TimeoutError):
        await gateway.step(episode_id="e1", action=SlowAction(sleep=999))
```

### 覆盖率门禁与有用的报告

```bash
pytest --cov=runner --cov-report=term-missing --cov-fail-under=85
#   --cov-report=term-missing 列出未覆盖的行号（指导补测试）
#   --cov-fail-under=85       覆盖率低于 85% 直接 fail CI
```

但**覆盖率是 necessary not sufficient**：100% 覆盖率仍可能断言空洞。把覆盖率当"哪里完全没测"的雷达，而非质量勋章。配合 `pytest -x`（首个失败即停）、`pytest --lf`（只跑上次失败的）、`pytest -n auto`（pytest-xdist 并行）让本地循环飞快。property-based 测试用 `hypothesis` 自动生成边界输入，对解析器/序列化这类纯函数尤其值——它会找到你想不到的反例。

## 43a.2 打包与分发：让代码可被 import 与安装

适配层、轨迹工具不能永远是"一堆脚本"。要让 RL 团队 `uv add your-harness-client` 就能用，需要标准的包结构与发布流程。第 40 章用了 `uv`，这里讲清楚 `pyproject.toml` 这份单一事实来源。

```toml
# pyproject.toml —— 现代 Python 项目的唯一配置文件（PEP 621）
[project]
name = "harness-runner"
version = "0.3.0"
description = "Agent rollout runner & eval adapters"
requires-python = ">=3.12"
dependencies = [
  "pydantic>=2.7",
  "httpx>=0.27",
  "anthropic>=0.40",
]

[project.optional-dependencies]            # 可选依赖组：pip install harness-runner[langgraph]
langgraph = ["langgraph>=0.2"]
dev = ["pytest>=8", "pytest-cov", "pytest-mock", "ruff", "mypy"]

[project.scripts]                          # 装包后获得 `harness-run` 命令行入口
harness-run = "runner.main:cli"

[build-system]                             # 怎么把源码构建成 wheel
requires = ["hatchling"]
build-backend = "hatchling.build"

[tool.ruff]                                # lint + format 合一（替代 flake8+isort+black）
line-length = 100
[tool.mypy]
strict = true                              # 等价 TS 的 strict / Rust 的编译器
```

构建与发布：

```bash
uv build                       # 产出 dist/harness_runner-0.3.0-py3-none-any.whl + .tar.gz
uv publish --token $PYPI_TOKEN # 发到 PyPI（内部包发到私有 index）
# 验证可复现安装：在干净环境装 wheel 跑冒烟测试
uv run --isolated --with dist/*.whl harness-run --version
```

**wheel vs sdist**：wheel 是预构建的二进制分发（装得快、无需编译），sdist 是源码包（兜底，能在任何平台重新构建）。纯 Python 包两者都发即可。

**锁定与可复现**：`uv lock` 生成 `uv.lock`（等价 `Cargo.lock`/`pnpm-lock.yaml`），钉死整棵依赖树的精确版本与哈希。**应用进 git 锁文件，库不锁只声明范围**——和 Rust 生态同理。CI 用 `uv sync --frozen` 拒绝任何未锁定的漂移。

**应用打包进容器**：runner 是应用不是库，最终形态是镜像（第 45、47 章）。多阶段构建 + `uv sync --frozen` 锁定依赖，是 RL rollout worker 可复现的地基——环境漂移会让训练曲线无声崩坏（第 43 章铁律之三）。

## 43a.3 性能与并发：GIL、多进程与剖析

Python 慢有两个层次，要分清才能对症下药：单线程纯计算慢（解释器开销），以及**GIL 让多线程无法并行 CPU**。

### GIL 真相与三条出路

GIL（全局解释器锁）保证同一时刻只有一个线程执行 Python 字节码。后果：

| 工作负载 | GIL 影响 | 正确武器 |
|---|---|---|
| IO 密集（网络、磁盘、子进程） | **无影响**——等 IO 时释放 GIL | `asyncio`（第 41 章）/ 线程池 |
| CPU 密集（解析、计算、压缩） | 多线程无法加速 | `multiprocessing` / `ProcessPoolExecutor` |
| 调外部库（numpy、orjson、Rust 扩展） | 库内部可释放 GIL | 用原生扩展，GIL 不再是瓶颈 |

这正是第 43 章"rollout gateway 用 Rust 写"的理由：单机维持几千并发沙箱会话，Python 的 GIL + 内存开销撑不住，而 tokio 天生胜任。但 runner 侧（领任务、起沙箱、收轨迹）是 IO 密集，asyncio 完全够用——**选语言要按负载性质，不是按喜好**。

```python
# IO 密集：asyncio 高并发领任务 + 跑 episode（数千并发，单进程）
async def run_batch(tasks: list[TaskSpec], concurrency: int = 64):
    sem = asyncio.Semaphore(concurrency)          # 限流，别压垮沙箱池
    async def one(t: TaskSpec):
        async with sem:
            return await run_episode(t)
    return await asyncio.gather(*(one(t) for t in tasks))

# CPU 密集：多进程绕开 GIL（如离线批量重算 reward / 压缩轨迹）
from concurrent.futures import ProcessPoolExecutor
def recompute_rewards(trajectories: list[Path]) -> list[float]:
    with ProcessPoolExecutor() as pool:          # 默认进程数 = CPU 核数
        return list(pool.map(_score_one_traj, trajectories))
```

> Python 3.13+ 提供了实验性的 free-threaded（no-GIL）构建，长期可能改变上表。但生产以"按负载选并发模型"为准，不要赌实验特性。

### 剖析：定位真瓶颈再优化

和前端一样，**测量优先**。

```bash
python -m cProfile -o prof.out -m runner.main   # 函数级耗时
python -m pstats prof.out                        # 交互式看 cumtime 排序
py-spy top --pid <pid>                           # 采样式剖析线上进程，零侵入、不停服
py-spy dump --pid <pid>                          # 卡住时抓所有线程栈（排查死锁神器）
```

`py-spy` 是生产排障利器：不改代码、不重启，直接对运行中的 worker 采样出火焰图。常见优化：JSON 用 `orjson`（比标准库快数倍，轨迹序列化热点）；大文件用生成器流式处理（第 40 章，不爆内存）；pydantic 校验在热路径上用 `model_construct`（跳过校验，仅当数据已可信）。但记住——**80% 的性能问题是 N+1 查询、没加索引、整文件读进内存这类结构性错误，而非 Python 本身慢**。

## 43a.4 与 Rust 互操作的工程边界

第 41.5 提过互操作。生产里"Rust 写性能核心、Python 做编排"的分工有三种落地：进程边界（HTTP/gRPC，最松耦合，第 43 章 gateway 用的就是这个）、`PyO3`+`maturin` 把 Rust 编译成 Python 扩展模块（`import` 即用，零拷贝、释放 GIL）、共享内存/Arrow（大数据零拷贝）。选哪种看耦合度与数据量：rollout gateway 跨机器 → HTTP；本地 token 化/打分热点 → PyO3 扩展。

```python
# maturin 构建的 Rust 扩展，对 Python 侧是普通模块
from harness_native import tokenize_fast   # 实际是 Rust 实现
ids = tokenize_fast(text)                  # 释放 GIL，可多线程并行调用
```

## 43a.5 本章小结与练习

- pytest 工程化：fixture 做依赖注入与分作用域清理、parametrize 消重、mock 只打边界、异步测试不漏 await、覆盖率做雷达而非勋章；`hypothesis` 给纯函数找反例。
- 打包：`pyproject.toml` 是单一事实来源，`uv build/publish` 发包，应用锁 `uv.lock` 进 git，最终以容器为分发形态。
- 性能：先分清 IO 密集（asyncio）还是 CPU 密集（multiprocessing），GIL 决定武器；`py-spy` 零侵入剖析线上进程；多数瓶颈是结构性错误不是语言。
- 互操作按耦合与数据量选：跨机用 HTTP/gRPC，本地热点用 PyO3 扩展。

**练习**

1. 给 `agent-eval-platform/runner` 写一套 pytest：fixture 提供临时仓库 + MockAdapter，参数化覆盖 5 种事件解析，用 `mocker` 假冒 LLM 验证工具错误重试路径，覆盖率门禁 `--cov-fail-under=85`。
2. 把 runner 打成可发布包：补全 `pyproject.toml`、`uv build` 产出 wheel、在 `--isolated` 干净环境装包跑冒烟测试，并写一个 `harness-run` CLI 入口。
3. 用 `py-spy` 剖析一个故意写得低效的轨迹批处理脚本（整文件读 + 标准库 json + 单进程），分别用生成器、orjson、ProcessPoolExecutor 优化，记录三步各自的加速比。
4. 设计实验对比 asyncio 与 ProcessPoolExecutor 在"并发跑 100 个 mock episode"（IO 密集）和"重算 100 条轨迹 reward"（CPU 密集）两种负载下的耗时，验证 43a.3 的选型表。
