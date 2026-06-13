# 第 36 章 React 进阶与前端工程化

> 上一章你会用 useState/useEffect 写完整应用了。但真实项目还需要：把重复逻辑抽出来复用、合理地组织各种状态、配好工程目录、做页面跳转、应对大数据量。本章把这些一个个补上。我的讲法是**每个工具先告诉你它为解决什么痛点而生**，再看怎么用——理解"为什么"比记住"怎么用"重要得多。

## 36.1 自定义 Hook：把可复用逻辑打包

先看一个痛点。第 35 章那段"拉数据 + loading/error 三态"的逻辑，几乎每个需要数据的组件都要写一遍。重复代码是 bug 的温床。

React 的解决办法叫**自定义 Hook**——把一段用到 useState/useEffect 的逻辑抽成一个函数（名字必须以 `use` 开头），多个组件就能复用：

```tsx
import { useState, useEffect } from "react";

// 把"拉数据三态"打包成一个可复用的 Hook
function useFetch<T>(url: string) {
  const [data, setData] = useState<T | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setLoading(true);
    fetch(url)
      .then((r) => { if (!r.ok) throw new Error(`HTTP ${r.status}`); return r.json(); })
      .then(setData)
      .catch((e) => setError(e.message))
      .finally(() => setLoading(false));
  }, [url]);

  return { data, loading, error };
}

// 用起来一行搞定，任何组件都能复用
function RunList() {
  const { data: runs, loading, error } = useFetch<RunSummary[]>("/api/runs");
  if (loading) return <p>加载中…</p>;
  if (error) return <p>出错：{error}</p>;
  return <ul>{runs?.map((r) => <li key={r.id}>{r.task}</li>)}</ul>;
}
```

自定义 Hook 是 React 复用逻辑的标准方式。记住它的本质：**就是一个普通函数，只不过内部用了别的 Hook**。

顺便记住用 Hook 的两条铁律，违反会出诡异 bug：

| 铁律 | 为什么 |
|---|---|
| 只在组件顶层调用 Hook，不要放进 if / for 里 | React 靠"调用顺序"记住每个 state，顺序乱了状态就串位 |
| useEffect 的依赖数组要列全用到的变量 | 漏了会读到过期旧值（闭包陷阱的 React 版） |

还有个常用 Hook 叫 `useRef`，两个用途：抓住某个真实 DOM 元素（比如让某个输入框聚焦）；存一个"变了也不需要重画"的值（比如一个 WebSocket 连接对象）。

## 36.2 状态分层：不同的状态用不同的工具

新手常问"状态管理到底用什么"。2026 年的共识是**分层**——不同种类的状态用不同工具，而不是一股脑全塞进一个大库。先看这张分层表，再逐个讲：

| 状态种类 | 例子 | 用什么 |
|---|---|---|
| 服务端数据 | runs 列表、轨迹详情 | **TanStack Query** |
| 全局客户端状态 | 当前选中哪个 run、主题 | **Zustand** |
| 局部 UI 状态 | 输入框内容、折叠开关 | `useState`（上一章） |
| URL 状态 | 过滤条件、分页 | 路由的 search params |

### 36.2.1 服务端数据用 TanStack Query

痛点：第 35 章手写的 `useFetch` 其实很简陋——它不会缓存（每次进页面都重新请求）、不会自动重试、不会在窗口重新聚焦时刷新、多个组件请求同一数据会重复发。这些都自己写会非常繁琐。

TanStack Query 专门解决"管理服务端数据"的所有这些问题。它的核心认知是：**后端数据的"真相"在服务器，前端只是它的一份缓存**。

```tsx
import { useQuery } from "@tanstack/react-query";

function useRunEvents(runId: string) {
  return useQuery({
    queryKey: ["runs", runId, "events"],     // 这份数据的唯一标识（缓存的钥匙）
    queryFn: async () => {
      const res = await fetch(`/api/runs/${runId}/events`);
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      return res.json();
    },
    staleTime: 30_000,                        // 30 秒内认为数据新鲜，不重复请求
    refetchInterval: 2_000,                   // 每 2 秒轮询一次（看运行中的批次进度）
  });
}

function TracePage({ runId }: { runId: string }) {
  const { data, isLoading, error } = useRunEvents(runId);
  // loading/error/data 三态它都帮你管好了，你只管用
  if (isLoading) return <p>加载中…</p>;
  if (error) return <p>出错</p>;
  return <Timeline events={data} />;
}
```

为什么它特别适合 Agent 评测平台：批次是长任务，`refetchInterval` 能轮询进度；多个组件订阅同一数据会自动合并成一次请求；收到"批次完成"的消息时可以精确让对应缓存失效、自动刷新。**前端数据层的现代共识：服务端数据交给 TanStack Query，别再手写 useEffect 拉数据。**

### 36.2.2 全局客户端状态用 Zustand

痛点：有些状态是"纯前端的"且要跨组件共享，比如"当前选中哪个 run""现在是暗色还是亮色主题"。用上一章的"状态提升"会导致一路 props 往下传很多层（叫 props drilling，很烦）。

Zustand 是个小巧的全局状态库，让任何组件都能直接读写共享状态，不用层层传递：

```tsx
import { create } from "zustand";

// 定义一个全局 store
const useUiStore = create<{
  selectedRunId: string | null;
  select: (id: string) => void;
}>((set) => ({
  selectedRunId: null,
  select: (id) => set({ selectedRunId: id }),
}));

// 任何组件里直接用，不用 props 传递
function RunRow({ id }: { id: string }) {
  const select = useUiStore((s) => s.select);     // 只取我要用的那部分
  return <div onClick={() => select(id)}>...</div>;
}
```

注意 `useUiStore((s) => s.select)` 只订阅 store 里需要的那一小块——这样别的部分变化时这个组件不会无谓重画，是性能上的好习惯。

### 36.2.3 URL 状态

像"当前过滤条件"这种，放进网址里有个额外好处：可分享。同事能直接把"失败的 runs 列表"链接发到群里，对方打开就是同样的过滤视图。这用路由的 search params 实现，下一节讲。

## 36.3 工程化：搭一个像样的项目骨架

随着应用变大，文件怎么组织很重要。推荐**按功能分目录**（而不是按文件类型把所有组件堆一起）：

```
trace-viewer/
├── src/
│   ├── api/              # 和后端打交道：fetch 封装 + Zod 校验规则
│   ├── features/         # 按"功能"分，每个功能自成一块
│   │   ├── runs/         #   运行列表：组件、Hook、测试放一起
│   │   ├── trace/        #   轨迹查看器（第 38 章）
│   │   └── dashboard/    #   仪表盘
│   ├── stores/           # Zustand 全局状态
│   ├── components/       # 通用小组件（Button、Badge 等）
│   ├── App.tsx           # 组装路由和各种 Provider
│   └── main.tsx          # 入口
├── vite.config.ts        # Vite 配置
└── tsconfig.json         # TypeScript 配置
```

`vite.config.ts` 里有个开发时的实用配置——把 `/api` 请求**代理**到你的 Rust 后端，这样前端（5173 端口）调后端（8080 端口）时不会撞上浏览器的跨域限制（CORS）：

```typescript
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  server: {
    proxy: {
      // 前端发给 /api 的请求，开发时自动转发到本地 8080 的后端
      "/api": { target: "http://localhost:8080", changeOrigin: true },
    },
  },
});
```

## 36.4 路由：多个页面之间跳转

单页应用其实只有一个 HTML，"翻页"是用 JS 切换显示哪个组件、同时改地址栏。这件事用 React Router 做：

```tsx
import { createBrowserRouter, RouterProvider } from "react-router-dom";

const router = createBrowserRouter([
  { path: "/",              element: <DashboardPage /> },   // 首页
  { path: "/runs",          element: <RunListPage /> },     // 列表页
  { path: "/runs/:runId",   element: <TracePage /> },       // 详情页，:runId 是变量
]);

function App() {
  return <RouterProvider router={router} />;
}
```

在详情页里用 `useParams` 拿到地址里的 `runId`；用 `useSearchParams` 读写 `?status=failure` 这种过滤参数（这就是上面说的"URL 状态"）：

```tsx
const { runId } = useParams();                  // 从 /runs/run-42 拿到 "run-42"
const [params] = useSearchParams();
const status = params.get("status") ?? "all";   // 从 /runs?status=failure 拿过滤条件
```

**代码分割**：详情页可能很大（带图表库等）。可以让它"用到时才加载"，加快首页打开速度：

```tsx
import { lazy, Suspense } from "react";
const TracePage = lazy(() => import("./features/trace/TracePage"));  // 懒加载

// 用 Suspense 包住，加载期间显示占位
<Suspense fallback={<p>加载中…</p>}>
  <TracePage />
</Suspense>
```

## 36.5 性能：上万条数据也不卡

痛点：一次 Agent 运行可能有 5000+ 条事件。如果老老实实把 5000 个 DOM 元素都画出来，页面会卡死。三个应对手段：

**① 虚拟列表（最有效）**：屏幕一次只显示几十行，那就**只画可视区域内的那几十行**，滚动时动态替换。哪怕数据有一万条，DOM 里始终只有几十个元素。第 38 章会实战，库用 `@tanstack/react-virtual`。

**② memo：跳过不必要的重画**。默认情况下父组件重画会带着子组件一起重画。用 `memo` 包住的组件，只有它自己的 props 变了才重画：

```tsx
import { memo } from "react";

// props（event）没变时，这个组件就不重画，省下开销
const EventRow = memo(function EventRow({ event }: { event: TraceEvent }) {
  return <div className="event-row">{describe(event)}</div>;
});
```

**③ 流式数据节流**：Agent 的 token 可能每秒蹦几十个，每个都触发一次重画太浪费。把它们攒一小批（比如 100 毫秒一批）再统一更新：

```tsx
// 攒够 100ms 再一次性更新，而不是每个 chunk 更新一次
const flush = setInterval(() => {
  if (buf.length) { setEvents((prev) => [...prev, ...buf]); buf = []; }
}, 100);
```

**最重要的原则：先测量，再优化。** 用浏览器里的 React DevTools 的 Profiler 录一段操作，看到底哪个组件重画最多、为什么，再针对性优化。盲目到处加 `memo` 反而会因为多了比较开销而变慢。

## 36.6 组件测试

改了代码怕改坏，就需要测试。React 组件测试用 Testing Library，它的哲学是**像用户一样测**——通过"用户看到的文字、点的按钮"来操作和断言，而不是检查组件内部状态：

```tsx
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

it("输入关键词能过滤列表", async () => {
  render(<RunList runs={fixtures} />);
  // 像用户一样：找到输入框、打字
  await userEvent.type(screen.getByRole("textbox"), "修复");
  // 像用户一样：断言看到的结果
  expect(screen.getAllByRole("listitem")).toHaveLength(2);
});
```

这样写的测试，即使你将来重构了组件内部实现，只要用户看到的行为没变，测试照样通过——这才是有价值的测试（第 38a 章会把测试讲到精通）。

## 36.7 小结与练习

- 自定义 Hook（`use` 开头的函数）把可复用逻辑打包；Hook 只在顶层调用、依赖数组要列全。
- 状态分层：服务端数据用 TanStack Query（自带缓存/轮询/重试）、全局客户端状态用 Zustand、局部用 useState、可分享的过滤条件放 URL。
- 工程化：按功能分目录、Vite 代理避开跨域；路由做多页面 + 懒加载分割代码。
- 性能：虚拟列表 > memo > 流式节流，且永远"先测量再优化"。
- 组件测试像用户一样测行为，不测内部实现。

**练习**

1. 用本章的工具搭一个 trace-viewer 骨架：`/runs` 列表页用 TanStack Query 拉数据 + URL 过滤 + 运行中每 2 秒轮询。
2. 把第 35 章的 `useFetch` 升级成自定义 Hook 库的一员，并在两个不同组件里复用它。
3. 给 RunList 写组件测试，覆盖"加载中""空列表""过滤后"三种情况。
4. 用 React DevTools Profiler 找出你的实现里重画最多的组件，用 `memo` 优化，记录优化前后的重画次数。

> **下一章**：流式 UI——让 Agent 的输出像打字机一样一个字一个字蹦出来。这是 Agent 前端最有特色的部分，我们会把后端（Rust）和前端连起来跑通。
