# 第 35 章 React 入门：从第一个组件开始

> 第 32 章你写过 `render()`——状态一变就清空页面重画。这个思路是对的，但手写它又累又容易出错。React 就是把这个"状态变了自动重画"的模式做成了工业级框架，是当今最主流的前端框架。本章零基础入门 React，目标是把第 32 章的轨迹页面用 React 重做一遍，你会亲眼看到它省了多少事。

## 35.1 React 的核心思想：UI = f(state)

先理解 React 最核心的一句话：**界面是数据的函数**，写成公式就是 `UI = f(state)`。

用 Excel 类比最直观：你在单元格里写公式 `=A1*2`，A1 改了，结果**自动**跟着变，你从不手动去改结果格。React 就是把整个网页变成这样一个"大公式"——你只描述"给定这份数据，界面应该长什么样"，数据一变，React **自动**帮你把界面更新到对应的样子。你再也不用像第 32 章那样手动 `querySelector` 去抓元素、手动改它。

对比一下两种思维方式：

| 第 32 章（手动挡） | React（自动挡） |
|---|---|
| 数据变了，我得记得去改界面 | 我只管改数据，界面自动跟着变 |
| 用 `querySelector` 一个个抓元素来改 | 不碰具体元素，只声明"界面该是什么样" |
| 容易出现"数据变了但界面忘了更新"的 bug | 界面永远和数据一致，这类 bug 不存在 |

React 有三个核心概念，本章逐个讲：

| 概念 | 一句话 | 类比 |
|---|---|---|
| **组件** | 一个返回"界面描述"的函数 | 你自定义的、可复用的"HTML 标签" |
| **JSX** | 在 JS 里写类似 HTML 的语法 | 描述界面长什么样 |
| **state** | 组件的"记忆"，一改它界面就重画 | Excel 里那个会触发重算的单元格 |

## 35.2 第一步：把项目跑起来

React 项目需要一套工具来打包。我们用 Vite（一个又快又现代的前端构建工具）一键生成：

```bash
pnpm create vite trace-viewer-react --template react-ts   # react-ts = React + TypeScript
cd trace-viewer-react
pnpm install      # 下载依赖
pnpm dev          # 启动！打开提示的 http://localhost:5173
```

打开后你会看到一个示例页面，而且**改代码保存后浏览器自动刷新**（这叫热更新，开发体验极好）。

> 模板带了 TypeScript（上一章学的）。本章代码里的类型标注都很轻，看不懂的标注先忽略，不影响理解 React。

刚生成的项目文件很多，但现在只需关心三个：

```
index.html        ← 唯一的 HTML 文件，里面有个空的 <div id="root">
src/main.tsx      ← 入口，负责把你的 App 塞进那个 root
src/App.tsx       ← 根组件，你的代码从这里写起
```

`main.tsx` 是固定的起手式，看一眼就好，基本不用动：

```tsx
import { createRoot } from "react-dom/client";
import App from "./App";

// 找到 index.html 里的 #root，把 <App /> 渲染进去
createRoot(document.getElementById("root")!).render(<App />);
```

## 35.3 组件与 JSX：自定义你的"标签"

**组件就是一个函数，它返回"界面长什么样"的描述。** 规则：函数名**必须大写开头**。

```tsx
// src/App.tsx
function App() {
  const runId = "run-42";
  const cost = 0.0312;

  return (
    <main>
      <h1>轨迹查看器</h1>
      {/* 大括号 = 从"写界面"切回"写 JS"，里面能放任何 JS 表达式 */}
      <p>当前运行：{runId}，成本 ${cost.toFixed(4)}</p>
      <p>{cost > 0.05 ? "⚠️ 超预算" : "✓ 预算内"}</p>
    </main>
  );
}
export default App;
```

函数里 `return` 的这一坨看着像 HTML 的东西，叫 **JSX**——它让你在 JS 里直接写界面。关键技巧：**用大括号 `{}` 在 JSX 里嵌入 JS**，变量、计算、三元判断都能放进去。

JSX 和 HTML 几乎一样，但有几条小差异，记住就行：

```tsx
<div className="event-row">           {/* 不是 class，是 className（class 是 JS 保留字） */}
<img src={url} />                      {/* 没有内容的标签必须自己闭合，加个 /> */}
<>                                     {/* 想返回多个并列元素，用这个空标签包起来 */}
  <td>a</td><td>b</td>
</>
```

> **JSX 背后发生了什么**（了解即可）：`<h1>你好</h1>` 其实会被转成一个普通 JS 对象，描述"一个 h1，内容是你好"。组件返回的就是一棵这样的"界面描述对象树"。React 拿这棵新树和上一次的对比，只把**变化的部分**更新到真实页面上——这就是它又快又不丢失输入焦点的秘密。

## 35.4 Props：给组件传参数

组件能复用的关键，是能像函数一样接收参数。React 里这些参数叫 **props**。

比如我们把"一行事件"做成一个可复用组件，每次用不同数据：

```tsx
// 定义组件，从 props 里解构出要用的字段
function EventRow({ tool, summary, ms, ok }: {
  tool: string; summary: string; ms: number; ok: boolean;
}) {
  return (
    <div className={ok ? "event-row" : "event-row failed"}>
      <span className="tool-name">{tool}</span>
      <span className="summary">{summary}</span>
      <span className="duration">{ms}ms</span>
    </div>
  );
}

// 使用它，像写 HTML 属性一样传参（非字符串的值用大括号）
function App() {
  return (
    <main>
      <EventRow tool="grep" summary="找到 divide" ms={8} ok={true} />
      <EventRow tool="bash" summary="测试失败" ms={2100} ok={false} />
    </main>
  );
}
```

一条重要规则：**props 是只读的，子组件绝不能修改收到的 props**。数据只能从父组件"往下流"给子组件，这叫**单向数据流**——它让数据的来源永远清晰，是 React 好维护的关键。

## 35.5 渲染列表

真实数据是一个数组，怎么把它渲染成一串组件？用第 32 章学的 `map`：

```tsx
function Timeline({ events }: { events: TraceEvent[] }) {
  if (events.length === 0) {
    return <p>暂无事件</p>;          // 数据为空时显示别的（叫"条件渲染"）
  }
  return (
    <div className="timeline">
      {events.map((e) => (
        <EventRow key={e.id} tool={e.tool} summary={e.summary} ms={e.ms} ok={e.ok} />
      ))}
    </div>
  );
}
```

注意那个 `key={e.id}`——**列表里每一项都必须有一个唯一的 `key`**。React 靠它在两次渲染间认出"哪一项还是原来那一项"。不写 key（或拿数组下标当 key）会在增删、排序时出现错位 bug，控制台也会警告你。务必用数据自身的稳定 id 当 key。

## 35.6 State：组件的"记忆"（最核心）

到目前为止界面都是"死"的。要让它能响应交互、会变化，就需要 **state（状态）**——组件的一块"记忆"，而且**一旦改变这块记忆，React 就自动重画这个组件**。

用 `useState` 给组件加一块 state：

```tsx
import { useState } from "react";

function Counter() {
  const [count, setCount] = useState(0);
  //     ↑当前的值  ↑改它的函数        ↑初始值

  return (
    <button onClick={() => setCount(count + 1)}>
      点击了 {count} 次
    </button>
  );
}
```

`useState(0)` 返回两样东西：当前的值 `count`，和一个用来改它的函数 `setCount`。点击按钮 → 调用 `setCount(count + 1)` → React 重新运行 `Counter` 函数 → 这次 `count` 是新值 → 界面更新。

**关键认知：`setCount(...)` 不是简单赋值，而是在对 React 说"数据变了，请帮我重画"。** 这正是 Excel 单元格自动重算的那个机制。

state 有三条规则，新手必须现在就建立，否则会遇到莫名其妙的 bug：

```tsx
// 规则 1：不可变更新 —— 永远造一个"新"的对象/数组，不要原地改旧的
const [events, setEvents] = useState<TraceEvent[]>([]);
setEvents([...events, newEvent]);          // ✓ 用展开造一个新数组
// events.push(newEvent); setEvents(events); // ✗ 改的是同一个数组，React 以为没变，不重画！

// 规则 2：基于旧值算新值时，用函数写法
setCount((c) => c + 1);                     // 比 setCount(count + 1) 更可靠

// 规则 3：set 之后，本次函数里读到的还是旧值（新值要等下次重画）
setCount(count + 1);
console.log(count);     // 打印的还是旧值！
```

规则 1 最重要，解释一下**为什么**：React 判断"数据变没变"，靠的是看"还是不是同一个对象"（比引用，不比内容）。你用 `push` 改原数组，它还是同一个对象，React 就以为啥都没变、不重画。所以必须每次都造个新的。记住这条，能省掉你将来无数小时的困惑。

**输入框的标准写法**叫"受控组件"——输入框显示什么，完全由 state 说了算：

```tsx
function SearchBox() {
  const [keyword, setKeyword] = useState("");
  return (
    <input
      value={keyword}                                // state 决定显示什么
      onChange={(e) => setKeyword(e.target.value)}   // 用户打字 → 更新 state
      placeholder="搜索…"
    />
  );
}
```

## 35.7 状态住在哪：状态提升

新问题：搜索框 `SearchBox` 里有 keyword，但需要过滤的 `Timeline` 是另一个组件，它怎么拿到 keyword？

答案：**把共享的 state 放到它们共同的父组件里，再用 props 往下分发**。这叫"状态提升"。

```tsx
function App() {
  const [keyword, setKeyword] = useState("");        // state 放在共同父级
  const [onlyFailed, setOnlyFailed] = useState(false);

  // 过滤结果是"算出来的"，直接在渲染时算，不要再单独存一份 state！
  const visible = EVENTS
    .filter((e) => !onlyFailed || !e.ok)
    .filter((e) => e.tool.includes(keyword) || e.summary.includes(keyword));

  return (
    <main>
      <Toolbar
        keyword={keyword} onKeyword={setKeyword}     // 把值和"改它的函数"都传下去
        onlyFailed={onlyFailed} onOnlyFailed={setOnlyFailed}
      />
      <Timeline events={visible} />
    </main>
  );
}
```

这段浓缩了 React 的设计哲学，值得反复看：

- **一份数据只存一处**（keyword 只在 App 里有一份），界面处处一致；
- **能算出来的别另存**：`visible` 是从 state 算出来的，新手最大的坑就是再 `setVisible(...)` 存一份——两份数据迟早对不上；
- **数据向下传、事件向上报**：父组件把值传给子组件显示，子组件通过父组件给的函数把"用户的操作"报上来。

## 35.8 useEffect：和外部世界打交道

React 组件函数应该是"纯"的——给定相同的数据，就返回相同的界面，不干别的。那像"发网络请求""设定时器"这种事（统称"副作用"）放哪？用 `useEffect`：它的意思是"**这个组件渲染完之后，去做点别的事**"。

最常见的用途是组件出现时去后端拉数据：

```tsx
import { useState, useEffect } from "react";

function RunList() {
  const [runs, setRuns] = useState<RunSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    fetch("/api/runs")
      .then((res) => {
        if (!res.ok) throw new Error(`HTTP ${res.status}`);   // 第 33 章的坑
        return res.json();
      })
      .then(setRuns)
      .catch((e) => setError(e.message))
      .finally(() => setLoading(false));
  }, []);   // ← 这个空数组很关键，下面解释

  // 异步界面的标准三态：加载中 / 出错 / 正常
  if (loading) return <p>加载中…</p>;
  if (error) return <p>出错了：{error}</p>;
  return <ul>{runs.map((r) => <li key={r.id}>{r.task}</li>)}</ul>;
}
```

`useEffect` 的结构和那个末尾的"依赖数组"是理解它的关键：

```tsx
useEffect(() => {
  // 要做的副作用（渲染完后执行）
}, [依赖项]);
//  []        → 只在组件首次出现时执行一次（最常见）
//  [runId]   → 每当 runId 变化时重新执行
//  不写数组   → 每次重画都执行（几乎都是 bug，别这么写）
```

**依赖数组要"诚实"**：effect 里用到了哪些 state/props，就要把它们列进数组里，否则会读到过期的旧值（这正是第 33 章闭包陷阱的 React 版本）。装一个叫 `eslint-plugin-react-hooks` 的工具能自动帮你检查，它的警告别无视。

最后记住那个 **loading / error / data 三态**——所有"从后端拉数据"的界面都长这样，养成肌肉记忆：**有 fetch，就配三态**。

## 35.9 综合实战：React 版轨迹查看器

把前面所有东西拼成完整应用。写 React 的第一步永远是**先画组件树**（想清楚拆成哪些组件、数据放哪）：

```
App                  ← state 都放这: events, keyword, onlyFailed, selectedId
├── Toolbar          ← 收到过滤条件 + 修改函数
├── Timeline         ← 收到过滤后的 events
│   └── EventRow × N ← 点击时通知 App "我被选中了"
└── Inspector        ← 收到当前选中的那个 event
```

App 骨架（其余组件留给你按前面学的补全）：

```tsx
import { useState, useEffect } from "react";

export default function App() {
  const [events, setEvents] = useState<TraceEvent[]>([]);
  const [loading, setLoading] = useState(true);
  const [keyword, setKeyword] = useState("");
  const [onlyFailed, setOnlyFailed] = useState(false);
  const [selectedId, setSelectedId] = useState<number | null>(null);

  useEffect(() => {
    fetch("/events.json")              // 把第 32 章的数据文件放进 public/ 目录
      .then((r) => r.json())
      .then(setEvents)
      .finally(() => setLoading(false));
  }, []);

  // 全是"算出来的"，不另存 state
  const visible = events
    .filter((e) => !onlyFailed || !e.ok)
    .filter((e) => e.tool.includes(keyword) || e.summary.includes(keyword));
  const selected = events.find((e) => e.id === selectedId) ?? null;

  if (loading) return <p>加载中…</p>;
  return (
    <div className="app">
      <main className="timeline-pane">
        <Toolbar keyword={keyword} onKeyword={setKeyword}
                 onlyFailed={onlyFailed} onOnlyFailed={setOnlyFailed} />
        <Timeline events={visible} selectedId={selectedId} onSelect={setSelectedId} />
      </main>
      <Inspector event={selected} />
    </div>
  );
}
```

对比第 32 章手写版，体会三个质变：

1. **没有任何 `querySelector`**——你不再亲手指挥页面，只声明"界面应该是什么样"；
2. 选中、过滤、数据全在 state 里，界面是它们算出来的结果，**不可能**出现"数据和界面对不上"；
3. `EventRow` 这种组件可以直接搬到第 38 章的生产级查看器里复用——组件就是可复用的资产。

## 35.10 小结与练习

- React 的核心是 `UI = f(state)`：你改数据，界面自动跟着变，像 Excel 公式。
- 组件是返回 JSX 的函数（大写开头）；props 只读、数据向下流；列表渲染要给唯一 `key`。
- state 是会触发重画的"记忆"；三规则：不可变更新（造新的别改旧的）、函数式更新、set 后本次读到的还是旧值；能算出来的数据别另存 state。
- `useEffect` 管副作用（如 fetch）；依赖数组要诚实；异步界面配 loading/error/data 三态。

**练习**

1. 完成 35.9 的全部组件，再加：按"轮次"分组显示、点表头排序、主题切换按钮。
2. 给 Inspector 加一个"复制 JSON"按钮：点击后用 `navigator.clipboard.writeText` 复制，按钮文字变成"已复制 ✓"，两秒后还原（练 state + setTimeout）。
3. 用第 33 章练习 3 的 `mockEventStream` 代替 fetch，让事件每 500 毫秒到达一个、列表实时增长——你已经在写流式 UI 了（第 37 章预演）。
4. 故意制造两个 bug 看现象：(a) 用数组下标当 key，然后往列表头部插入一项；(b) effect 里用了 keyword 但依赖数组写成 `[]`。能解释清楚现象，才算真懂 key 和依赖数组。

> **下一章**：把 React 用到工程级——自定义 Hook、状态分层管理、路由、性能优化与测试，搭出一个能扛生产的前端骨架。
