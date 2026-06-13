# 第 38 章 补充 · 前端精通：测试金字塔、可访问性与性能工程

> 前面 8 章你已能独立写出轨迹查看器。但"能跑"和"专家级"之间隔着三件事：**测试金字塔**（改代码不慌）、**可访问性**（不是合规摆设，而是键盘党算法工程师每天在用）、**性能工程**（万级事件不卡、首屏不慢）。本章把这三块补到生产团队的水准。读完你具备的是 senior 前端的判断力，而不止是写组件的手速。

## 38a.1 测试金字塔：写哪种、写多少

```
        ╱╲          E2E (Playwright)        少：关键用户路径，慢但真实
       ╱──╲         集成/组件 (RTL)          中：组件 + 交互 + 状态
      ╱────╲        单元 (Vitest)           多：纯函数、reducer、解析器
     ╱──────╲
```

原则：**测试行为，不测实现**。断言"用户看到失败的 run 高亮成红色"，而不是"`useState` 被调用了一次"。实现细节会变，行为契约不变——后者才值得用测试锁住。第 34、36 章已写过 Vitest 单测与一个 RTL 组件测试，这里补齐金字塔的中层（组件交互）与顶层（E2E），并讲清楚每层的边界。

## 38a.2 组件测试进阶：React Testing Library

RTL 的哲学是"像用户一样找元素、像用户一样操作"。两条铁律：用**可访问性查询**（`getByRole`/`getByLabelText`）而非 `getByTestId`（除非万不得已）——这逼着你写出可访问的 DOM；所有交互走 `userEvent`（模拟真实事件序列：focus→keydown→input），而非 `fireEvent`（只派发单个合成事件）。

```tsx
// TraceTimeline.test.tsx —— 测一个有过滤 + 展开交互的组件
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { TraceTimeline } from "./TraceTimeline";
import type { TraceEvent } from "../api/schemas";

const fixtures: TraceEvent[] = [
  { type: "tool_call", turn: 1, toolName: "grep", args: {}, callId: "c1", ts: 1 },
  { type: "tool_result", callId: "c1", output: "src/math.rs:12", isError: false, durationMs: 8, ts: 2 },
  { type: "tool_result", callId: "c2", output: "1 failed", isError: true, durationMs: 2100, ts: 4 },
];

test("过滤到只看失败事件", async () => {
  const user = userEvent.setup();
  render(<TraceTimeline events={fixtures} />);

  // 初始：3 个事件行（用 role 定位，list/listitem 是语义化的回报）
  expect(screen.getAllByRole("listitem")).toHaveLength(3);

  // 勾选 "只看失败"
  await user.click(screen.getByRole("checkbox", { name: /只看失败/ }));

  const rows = screen.getAllByRole("listitem");
  expect(rows).toHaveLength(1);
  // 在该行内部断言（within 限定查询范围，避免误命中）
  expect(within(rows[0]).getByText(/1 failed/)).toBeInTheDocument();
});

test("点击事件行展开详情面板", async () => {
  const user = userEvent.setup();
  render(<TraceTimeline events={fixtures} />);
  await user.click(screen.getByRole("button", { name: /grep/ }));
  // 详情用 region 角色 + aria-label，测试和读屏软件读到的是同一个锚点
  expect(screen.getByRole("region", { name: /事件详情/ })).toHaveTextContent("src/math.rs:12");
});
```

**异步与等待**：永远用 `findBy*`（自带重试，返回 Promise）或 `waitFor`，不要 `setTimeout`。`findBy*` 会轮询直到元素出现或超时，天然适配"fetch 完成后渲染"。

```tsx
test("加载远程轨迹后渲染", async () => {
  render(<TracePage runId="r1" />);
  // 先看到 loading 占位
  expect(screen.getByRole("status")).toHaveTextContent(/加载中/);
  // 等数据到达后行出现（findBy 自动重试，无需手写延时）
  expect(await screen.findByText(/cargo test/)).toBeInTheDocument();
});
```

### Mock 网络层：MSW

不要 mock `fetch` 本身（脆、和实现耦合）。用 **MSW**（Mock Service Worker）在网络层拦截，前端代码完全不知道自己在被测——同一套 handler 还能给 Storybook 和本地开发复用。

```ts
// test/server.ts
import { setupServer } from "msw/node";
import { http, HttpResponse } from "msw";

export const server = setupServer(
  http.get("/api/runs/:id/events", ({ params }) => {
    if (params.id === "missing") return new HttpResponse(null, { status: 404 });
    return HttpResponse.json(fixtures);
  }),
);

// vitest.setup.ts
import { server } from "./test/server";
beforeAll(() => server.listen({ onUnhandledRequest: "error" })); // 未 mock 的请求直接报错，杜绝漏网
afterEach(() => server.resetHandlers());
afterAll(() => server.close());
```

`onUnhandledRequest: "error"` 是专家级细节：任何测试里发出的、你没显式 mock 的请求都会让测试失败——逼你显式声明每一次网络交互，把"测试偷偷打到真后端"这类幽灵 bug 扼杀在源头。

## 38a.3 E2E 测试：Playwright

E2E 测真实浏览器里的完整路径，只写**少数关键流**（提交批次→看到进度→打开失败 run→定位失败 turn）。多了会慢且脆。

```ts
// e2e/trace-flow.spec.ts
import { test, expect } from "@playwright/test";

test("从批次列表钻取到失败的 turn", async ({ page }) => {
  await page.goto("/batches/b-123");

  // 用户视角定位：role + 可见文本，和 RTL 同源心智
  await expect(page.getByRole("heading", { name: /批次 b-123/ })).toBeVisible();

  // 点开第一个失败的 run
  await page.getByRole("row", { name: /failed/ }).first().getByRole("link").click();

  // 进入 trace 页，等 SSE/数据加载完成（web-first 断言自动重试到超时）
  await expect(page.getByRole("region", { name: /timeline/ })).toBeVisible();

  // 筛选失败事件并展开
  await page.getByRole("checkbox", { name: /只看失败/ }).check();
  await page.getByRole("listitem").first().click();
  await expect(page.getByRole("region", { name: /事件详情/ })).toContainText("cargo test");
});

test("流式输出实时增长", async ({ page }) => {
  await page.goto("/runs/r-live");
  const stream = page.getByTestId("token-stream");
  await expect(stream).toContainText("分析", { timeout: 10_000 });
  // 断言"持续增长"：两次快照长度递增（流式 UI 的专属验证）
  const a = (await stream.textContent())!.length;
  await page.waitForTimeout(500);
  const b = (await stream.textContent())!.length;
  expect(b).toBeGreaterThan(a);
});
```

`playwright.config.ts` 的生产配置要点：`webServer` 自动起本地 dev server、`trace: "on-first-retry"`（失败时录制可回放的 trace，含 DOM 快照和网络——调 flaky 测试的神器）、`projects` 跑 Chromium/WebKit/Firefox 三引擎、`fullyParallel` 并行。把 `--ui` 模式留给本地调试，CI 用 headless + 重试 2 次。

**反 flaky 纪律**：禁用 `waitForTimeout` 做同步（只用于上面那种"测增长"的特例）；一切等待交给 web-first 断言（`expect(locator).toBeVisible()` 内建重试）；测试间不共享状态（每个 test 自带 fixtures 或通过 API 预置数据再清理）。

## 38a.4 可访问性（a11y）：你的用户是键盘党

内部工具最容易忽视 a11y，但 Agent 评测平台的用户是算法工程师——重度键盘使用者，很多人开着读屏或高对比模式。a11y 做好 = 键盘可达 + 语义正确 + 对比达标，三件事而已。

**语义优先**：能用原生元素就别用 `div` 模拟。`<button>` 自带焦点、回车/空格触发、`role=button`；用 `<div onClick>` 你要手动补全这一切还容易补漏。

```tsx
// ✗ 不可访问：键盘 Tab 不到、读屏读不出、回车不触发
<div className="row" onClick={expand}>{toolName}</div>

// ✓ 可访问：原生语义 + 状态通过 ARIA 暴露给辅助技术
<li>
  <button
    aria-expanded={isOpen}
    aria-controls={`detail-${callId}`}
    onClick={expand}
  >
    {toolName}
  </button>
  {isOpen && (
    <section id={`detail-${callId}`} role="region" aria-label="事件详情">
      …
    </section>
  )}
</li>
```

**键盘导航**：列表用方向键、`Home`/`End` 跳转，遵循 [WAI-ARIA Authoring Practices](https://www.w3.org/WAI/ARIA/apg/) 的对应模式（这里是 grid/listbox 模式）。焦点管理是重灾区：打开 Modal 要把焦点移进去、`Esc` 关闭、关闭后焦点**还回触发它的按钮**，且焦点不能逃出 Modal（focus trap）。

```tsx
function Modal({ onClose, children }: { onClose: () => void; children: React.ReactNode }) {
  const ref = useRef<HTMLDivElement>(null);
  const opener = useRef<HTMLElement | null>(null);

  useEffect(() => {
    opener.current = document.activeElement as HTMLElement;   // 记住从哪来
    ref.current?.querySelector<HTMLElement>("[autofocus],button,a,input")?.focus();
    const onKey = (e: KeyboardEvent) => { if (e.key === "Escape") onClose(); };
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("keydown", onKey);
      opener.current?.focus();                                // 关闭后焦点还家
    };
  }, [onClose]);

  return (
    <div ref={ref} role="dialog" aria-modal="true" aria-labelledby="modal-title">
      {children}
    </div>
  );
}
```

**动态内容播报**：流式 token、"批次完成"这类异步更新，视觉用户看得到，读屏用户需要 `aria-live` 区域主动播报。

```tsx
// 礼貌播报（不打断当前朗读），适合状态变更
<div aria-live="polite" className="sr-only">{statusText}</div>
// .sr-only：视觉隐藏但读屏可读（绝对定位 + 1px 裁剪，别用 display:none）
```

**自动化兜底**：`axe-core` 抓得到 ~40% 的 a11y 问题（对比度、缺 label、ARIA 误用），接进测试：

```ts
import { axe } from "vitest-axe";
test("无可访问性违规", async () => {
  const { container } = render(<TraceTimeline events={fixtures} />);
  expect(await axe(container)).toHaveNoViolations();
});
```

剩下 60%（焦点顺序、播报时机、键盘陷阱）必须**手动测**：拔掉鼠标用 Tab/方向键走一遍，打开 VoiceOver(Mac `Cmd+F5`)/NVDA 听一遍。这是自动化替代不了的专家功课。

## 38a.5 性能工程：测量优先，别凭感觉优化

铁律：**先测量，再优化**。盲目加 `useMemo`/`memo` 会让代码更难读且收益为零甚至为负。流程是 profile → 定位真瓶颈 → 针对性修 → 复测确认。

### React 渲染剖析

React DevTools 的 **Profiler** 录一段交互，看火焰图：哪些组件渲染了、为什么（"why did this render"）、耗时多少。常见三类病灶与药方：

| 症状 | 根因 | 药方 |
|---|---|---|
| 输入框敲字全列表重渲染 | 父组件 state 变化波及所有子组件 | 拆分组件 + `React.memo` 隔离稳定子树 |
| 每次渲染子组件都变 | 内联 `{}`/`() =>` 每次新引用，破坏 memo | `useCallback`/`useMemo` 稳定引用 |
| 列表项全量重算 | 派生数据（排序/过滤）每次重算 | `useMemo` 缓存派生结果 |

```tsx
// 把昂贵派生计算 memo 化，依赖不变就不重算
const sorted = useMemo(
  () => [...events].sort((a, b) => a.ts - b.ts),
  [events],
);
// 传给 memo 子组件的回调要稳定，否则 memo 形同虚设
const onSelect = useCallback((id: string) => setSelected(id), []);
```

但**最有效的优化往往是结构性的**：第 38 章的虚拟滚动（只渲染视口内的行）让万级事件从卡死到丝滑，这比任何 memo 都管用。优化优先级：算法/数据结构 > 减少渲染量（虚拟化/分页）> 减少重渲染（memo）> 微优化。

### 加载性能与 Core Web Vitals

生产前端的体感由三个指标定义，DevTools Lighthouse 和 `web-vitals` 库都能测：

- **LCP**（最大内容绘制 < 2.5s）：首屏主内容多快可见。优化：代码分割（第 36 章 `React.lazy`）、按路由切包、关键资源 preload。
- **INP**（交互到下次绘制 < 200ms）：点击/输入多快有反馈。优化：拆长任务、`startTransition` 把非紧急更新降级、避免主线程阻塞。
- **CLS**（累积布局偏移 < 0.1）：内容跳动。优化：图片/骨架预留尺寸。

```tsx
// React 18 并发：把"过滤大列表"标为非紧急，输入保持跟手
const [query, setQuery] = useState("");
const [deferredQuery] = [useDeferredValue(query)];
const visible = useMemo(() => filterEvents(events, deferredQuery), [events, deferredQuery]);
// 输入框用 query（即时反馈），列表用 deferredQuery（可被高优先级输入打断）
```

### Bundle 体积治理

```bash
pnpm build && pnpm dlx vite-bundle-visualizer   # 可视化每个依赖占多大
```

专家手段：`import` 时按需引入（`import debounce from "lodash-es/debounce"` 而非整包）；重依赖（图表库、diff 库）用 `React.lazy` 懒加载到独立 chunk；用 `rollup-plugin-visualizer` 进 CI，bundle 超阈值就 fail（防止依赖悄悄膨胀）。

## 38a.6 服务端状态：TanStack Query

第 36 章讲的状态管理（useState/Context/Zustand）解决的是**客户端状态**。但 Agent 前端 80% 的状态其实是**服务端状态**——批次列表、run 详情、轨迹，它们的真相在后端，前端只是缓存。手写 `useEffect + useState` 管这些会陷入"loading/error/缓存/重试/失效"的泥潭。TanStack Query 是这个问题的标准答案。

```tsx
// 一行替代一大坨 useEffect 样板：自动管 loading/error/缓存/后台刷新
function useBatch(batchId: string) {
  return useQuery({
    queryKey: ["batch", batchId],
    queryFn: () => api.getBatch(batchId),
    staleTime: 30_000,           // 30s 内视为新鲜，不重复请求
    refetchOnWindowFocus: true,  // 切回标签页自动刷新（看实时进度的利器）
  });
}

function BatchPage({ id }: { id: string }) {
  const { data, isLoading, error } = useBatch(id);
  if (isLoading) return <Status>加载中…</Status>;
  if (error) return <ErrorView error={error} />;
  return <BatchDetail batch={data} />;
}
```

为什么对 Agent 平台尤其契合：评测批次是长任务，`refetchInterval` 可做轮询兜底（SSE 断线时降级）；`queryClient.invalidateQueries(["batch", id]）` 在收到"批次完成"SSE 事件时精确失效对应缓存；多个组件订阅同一 `queryKey` 自动去重请求。**客户端 UI 状态用 Zustand，服务端数据用 Query**——这条分界线是现代 React 数据层的共识。

## 38a.7 错误边界与降级

LLM 轨迹是不可信数据（第 19、32 章），一条畸形事件不该让整页白屏。用 Error Boundary 把崩溃隔离在局部：

```tsx
import { ErrorBoundary } from "react-error-boundary";

<ErrorBoundary
  fallbackRender={({ error, resetErrorBoundary }) => (
    <div role="alert">
      <p>这条轨迹渲染失败：{error.message}</p>
      <button onClick={resetErrorBoundary}>重试</button>
    </div>
  )}
  onError={(e) => reportToSentry(e)}   // 上报，第 15 章可观测性的前端侧
>
  <TraceTimeline events={events} />
</ErrorBoundary>
```

配合 `<Suspense>` 处理懒加载/数据加载的 pending 态，把"加载中/出错/正常"三态收敛成声明式结构，而不是散落各处的 `if (loading）`。

## 38a.8 本章小结与练习

- 测试金字塔：Vitest 测纯逻辑、RTL 测组件行为（role 查询 + userEvent + MSW）、Playwright 测关键路径（web-first 断言、trace 回放）；测行为不测实现。
- 可访问性是功能不是合规：语义化元素 + ARIA 状态 + 焦点管理 + `aria-live` 播报；axe 兜底 40%，键盘与读屏手测剩余 60%。
- 性能先测量：Profiler 定位重渲染、虚拟化 > memo、Core Web Vitals（LCP/INP/CLS）+ bundle 治理；并发特性（`useDeferredValue`/`startTransition`）保持跟手。
- 服务端状态交给 TanStack Query，客户端状态留给 Zustand；Error Boundary 隔离不可信轨迹的渲染崩溃。

**练习**

1. 给第 38 章的轨迹查看器补一套测试：≥5 个 Vitest 单测（事件解析/派生统计）、3 个 RTL 组件测试（过滤、展开、加载态用 MSW）、2 条 Playwright E2E（钻取失败 run、验证流式增长）。CI 里 `vitest run` + `playwright test` 全绿才允许合并。
2. 用键盘走查你的轨迹查看器，列出所有"鼠标能做但键盘做不到"的操作并修复；接入 `vitest-axe`，把现存违规清零。
3. 用 React DevTools Profiler 找出输入过滤时的多余重渲染，用 `useDeferredValue` + `memo` 优化，对比优化前后的火焰图截图。
4. 把 run 列表与详情改造成 TanStack Query，收到"批次完成"SSE 事件时精确 `invalidateQueries`，并实现 SSE 断线时降级为 5s 轮询。
