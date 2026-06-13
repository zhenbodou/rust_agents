# 第 38 章 实战：做一个专业级轨迹查看器

> 这一章是前端部分的总实战，也是你简历上的加分项本身——"做过 Agent 评测平台 / 轨迹回放 / 可视化工具"。轨迹查看器是 Agent 基础设施团队最常被要求做的内部工具，没有之一。我们把前面所有前端知识（组件、状态、流式、虚拟列表）汇成一个真东西。先想清楚"用户到底要看什么"，再一块块实现。

## 38.1 先做产品经理：用户要看什么

这个工具的用户是**调试 Agent 的算法工程师**。先搞清他们的痛点——他们盯着一个 Agent 反复问四个问题：

1. **它做了什么？** → 一条完整时间线：每轮想了什么、调了哪些工具、结果如何；
2. **它为什么这么做？** → 每一轮喂给模型的**完整输入**（也就是"模型当时看到的世界"）；
3. **哪里开始跑偏？** → 把成功的一次和失败的一次对比，找出分叉点；
4. **代价多大？** → 每轮花了多少 token、多少钱、多少时间。

这四个问题，正好对应我们要做的四个视图：**时间线、轮次详情、对比、统计**。先有需求，再写代码——这是做工具的正确顺序。

## 38.2 数据建模：把"流水账"重组成"结构"

后端为了写入方便，存的是一条条扁平的事件（流水账）。前端要先把它重组成"一轮一轮"的结构，才好展示。一轮（Turn）包含：这轮的模型输入、模型说的话、调用的工具及结果。

```typescript
interface Turn {
  index: number;                  // 第几轮
  assistantText: string;          // 这轮模型说的话（所有文字片段拼起来）
  toolCalls: ToolCallView[];      // 这轮调用的工具
  tokens: { input: number; output: number };
}

interface ToolCallView {
  call: ToolCallEvent;
  result?: ToolResultEvent;       // 工具的结果（按 callId 配对找到）
}

// 把扁平事件流，重组成一轮一轮的结构
function buildTurns(events: TraceEvent[]): Turn[] {
  const turns = new Map<number, Turn>();
  const pending = new Map<string, ToolCallView>();   // 等待结果的工具调用
  for (const e of events) {
    switch (e.type) {
      case "llm_request":
        turns.set(e.turn, { index: e.turn, assistantText: "", toolCalls: [],
                            tokens: { input: e.inputTokens, output: 0 } });
        break;
      case "llm_chunk":
        turns.get(e.turn)!.assistantText += e.delta;   // 把文字片段拼起来
        break;
      case "tool_call": {
        const view = { call: e };
        pending.set(e.callId, view);
        turns.get(e.turn)!.toolCalls.push(view);
        break;
      }
      case "tool_result": {
        const view = pending.get(e.callId);
        if (view) view.result = e;     // 用 callId 把"调用"和"结果"配对
        break;
      }
    }
  }
  return [...turns.values()].sort((a, b) => a.index - b.index);
}
```

**一个专业细节**：`buildTurns` 必须能容忍**损坏的轨迹**（Agent 进程被杀、事件丢了一半）。比如某个工具调用找不到对应结果，要显示成"⏳ 无结果"，而不是让整个页面崩溃。记住：**调试工具自己不能比被调试的东西还脆弱。**

## 38.3 时间线：上万条事件也不卡（虚拟滚动）

一次运行可能有几千上万条事件。如果老实把它们全画成 DOM 元素，浏览器会卡死。

解决办法叫**虚拟滚动**，原理用个类比就懂：你透过一扇窗户看一列很长的火车，窗户里一次只看得见几节车厢。那我们就**只渲染窗户里那几节**——屏幕上能看到几十行，就只画那几十行；用户滚动时，动态替换成新的几十行。哪怕数据有一万条，页面上始终只有几十个真实元素。

这个用现成的库 `@tanstack/react-virtual` 实现：

```tsx
import { useVirtualizer } from "@tanstack/react-virtual";

function Timeline({ rows }: { rows: Row[] }) {
  const parentRef = useRef<HTMLDivElement>(null);

  const virtualizer = useVirtualizer({
    count: rows.length,                          // 总共多少行
    getScrollElement: () => parentRef.current,
    estimateSize: () => 36,                       // 每行大概多高
    overscan: 12,                                 // 上下多画几行，滚动更顺
  });

  return (
    <div ref={parentRef} style={{ height: "100%", overflow: "auto" }}>
      {/* 用一个高度等于"全部行"的容器撑起滚动条 */}
      <div style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
        {/* 但只渲染当前可见的那些行 */}
        {virtualizer.getVirtualItems().map((vi) => (
          <div key={vi.key}
               style={{ position: "absolute", transform: `translateY(${vi.start}px)`, width: "100%" }}>
            <EventRow row={rows[vi.index]} />
          </div>
        ))}
      </div>
    </div>
  );
}
```

**视觉编码**（内部工具靠"信息密度"取胜，让人一眼看出问题）：不同工具用不同颜色图标（bash 灰、edit 黄、read 蓝）；失败的行整行标红；行尾右对齐显示耗时和 token，特别慢的标成橙色——**一眼看出慢在哪**。

## 38.4 轮次详情：看"输入"而非"输出"

调试 Agent 最反直觉、也最关键的一点：**要看的不是模型的输出，而是它的输入**。模型表现不好，几乎总是因为"喂给它的上下文"有问题（第 10 章 Context Engineering）。所以点开某一轮，要能看到当时喂给模型的完整内容：

```tsx
function TurnInspector({ turn, runId }: { turn: Turn; runId: string }) {
  // 完整输入快照可能很大，用 TanStack Query 惰性加载（点开才拉）
  const { data: snapshot } = useQuery({
    queryKey: ["runs", runId, "turn", turn.index],
    queryFn: () => fetchTurnRequest(runId, turn.index),
  });

  return (
    <Tabs>
      <Tab label="Prompt">
        {/* 把 system / 历史 / 工具定义分颜色显示 */}
        <PromptView messages={snapshot?.messages ?? []} />
      </Tab>
      <Tab label="原始 JSON">
        <CopyableJson data={snapshot} />   {/* 一键复制，可直接拿去重放 */}
      </Tab>
    </Tabs>
  );
}
```

两个最受算法同学欢迎的功能：

- **Token 占用条**：用一个横条显示这轮的上下文由什么组成（比如 system 占 12%、历史对话占 60%、工具定义占 18%）——上下文有没有被某部分撑爆，一目了然；
- **"从这里重放"**：把这一轮的输入快照发回后端、换个模型或改改 prompt 重跑一遍——调 prompt 时的 A/B 神器。

## 38.5 对比视图：找出两条轨迹的分叉点

评测回归分析的核心场景：同一个任务，旧模型过了、新模型挂了，到底差在哪一步？

朴素地"第一行对第一行"对比是不对的——两条轨迹可能前几步一样、中间某步开始分叉。正确做法是先**对齐**（找出两条轨迹里相同的步骤），再高亮第一个不同的地方：

```typescript
// 按"工具名 + 参数"判断两步是否相同，做对齐，找出分叉点
function alignTraces(a: Turn[], b: Turn[]): AlignedRow[] {
  const keyOf = (t: Turn) =>
    t.toolCalls.map((c) => `${c.call.toolName}:${normalizeArgs(c.call.args)}`).join("|");
  return lcsAlign(a, b, (x, y) => keyOf(x) === keyOf(y));
  // 每行结果是 "相同 / 只有A / 只有B / 分叉" 之一
}
```

界面做成左右双栏，相同的行对齐，**第一个"分叉"行自动高亮并滚动到那**——那个分叉点就是算法同学要找的答案。文件改动的对比（edit 工具的改前/改后）用现成的 `diff` 库做逐行高亮：

```tsx
import { diffLines } from "diff";

function FileDiff({ before, after }: { before: string; after: string }) {
  const parts = diffLines(before, after);
  return (
    <pre>
      {parts.map((p, i) => (
        <span key={i} style={{ background: p.added ? "#0d3" : p.removed ? "#d03" : "transparent" }}>
          {p.value}
        </span>
      ))}
    </pre>
  );
}
```

## 38.6 统计图表

用 Recharts（一个声明式的图表库，和 React 的数据流很搭）画图。比如"每轮的 token 消耗"柱状图：

```tsx
import { BarChart, Bar, XAxis, YAxis, Tooltip } from "recharts";

function TokensPerTurn({ turns }: { turns: Turn[] }) {
  const data = turns.map((t) => ({ turn: t.index, input: t.tokens.input, output: t.tokens.output }));
  return (
    <BarChart width={400} height={180} data={data}>
      <XAxis dataKey="turn" /><YAxis /><Tooltip />
      <Bar dataKey="input" stackId="t" fill="#58a6ff" />
      <Bar dataKey="output" stackId="t" fill="#3fb950" />
    </BarChart>
  );
}
```

**一个重要原则**：跨多个 run 的统计（通过率趋势、成本分布等），让**后端用 SQL 聚合好**再给前端画，别把十万行原始数据拉到浏览器里算——浏览器会卡死，这是新手常犯的错。

## 38.7 回放模式：给轨迹做个"录像机"

把一条轨迹变成可以播放、快进、拖动的"录像"。原理其实就是上一章学的派生状态：**显示哪些事件，由一个"游标"决定**，游标就是录像的进度。

```tsx
function usePlayback(events: TraceEvent[]) {
  const [cursor, setCursor] = useState(events.length);   // 当前播放到第几个事件
  const [playing, setPlaying] = useState(false);
  const [speed, setSpeed] = useState(1);                  // 1 倍速 / 4 倍速…

  useEffect(() => {
    if (!playing || cursor >= events.length) return;
    // 按事件之间的真实时间间隔回放（除以倍速），还原 Agent 当时的节奏
    const dt = Math.min((events[cursor].ts - events[cursor - 1].ts) / speed, 2000);
    const t = setTimeout(() => setCursor((c) => c + 1), dt);
    return () => clearTimeout(t);
  }, [playing, cursor, speed, events]);

  const visible = events.slice(0, cursor);   // 只显示游标之前的事件
  return { visible, cursor, setCursor, playing, setPlaying, speed, setSpeed };
}
```

在进度条上叠加一个"事件密度热力图"，并把失败点标红——用户能直接拖到红点附近，看出事前后到底发生了什么。

## 38.8 小结与练习

- 做工具先做产品经理：用户的四个问题对应四个视图（时间线、轮次详情、对比、统计）。
- 时间线用虚拟滚动（只画窗口里的几十行）扛住上万条事件；调试工具必须容忍损坏轨迹。
- 调 Agent 看的是**输入**不是输出；"从这里重放"和 token 占用条最受欢迎。
- 对比要先对齐再找分叉点；跨 run 统计让后端聚合、前端只画。
- 回放模式本质是"游标决定显示哪些事件"的派生渲染。

**练习**

1. 用 mini-claude-code 跑 10 次任务，把产生的 JSONL 轨迹导入你做的时间线 + 轮次详情，渲染出完整流程。
2. 实现 `alignTraces` 对齐算法，用两条手工构造的、中途分叉的轨迹验证它能正确定位分叉点。
3. 加上回放模式和密度热力图，要求 5000 条事件的轨迹拖动进度条依然流畅（每帧 < 16 毫秒）。

> **Part 7 到此完结**——你已经从"没写过一行网页"走到了能独立做出专业级 Agent 前端工具。Part 10 会把这些组件组装成完整的评测平台。下一部分我们转向后端与 Python，同样从零讲起。
