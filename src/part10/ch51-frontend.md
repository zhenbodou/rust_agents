# 第 51 章 前端实现：轨迹回放、对比与仪表盘

> 本章把第 31–38 章学的前端组件，组装成评测平台的界面 `agent-eval-platform/web/`。重点不是重复前面的代码，而是讲**真正组装时才会冒出来的新问题**——实时数据和历史数据如何无缝拼接、趋势图如何和后端统计 API 对接、对比报告如何用 URL 参数驱动状态。

## 51.1 类型安全：三端一致靠 Zod

第 49 章说过，事件模型用一份 JSON Schema 当"唯一真相"，这里是前端侧的落地。用 Zod 在运行时验证所有网络数据（`api/schemas.ts`）：

```typescript
// 关键原则：前端永远假设后端可能发来它不认识的新版本字段
export function parseEvent(raw: unknown): MaybeEvent {
  const r = TraceEventSchema.safeParse(raw);
  if (r.success) return r.data;
  // 解析失败 → 降级为"未知事件"占位，而不是崩溃
  return { type: "unknown", seq: (raw as any)?.seq ?? -1, raw };
}
```

API 客户端 (`api/client.ts`) 所有函数都用 Zod schema 验证响应，类型错误在网络层就能发现，不会流传到 UI 组件里变成莫名其妙的渲染错误。

**新增仪表盘和对比接口**：

```typescript
export const fetchDashboard = () =>
  get(`/api/stats/dashboard`, DashboardSchema);

export const fetchTrend = (days = 30, groupBy = "day") =>
  get(`/api/stats/trend?days=${days}&group_by=${groupBy}`,
      z.object({ points: z.array(TrendSchema) })).then(r => r.points);

export const fetchCompare = (a: string, b: string) =>
  get(`/api/reports/compare?a=${a}&b=${b}`, CompareReportSchema);
```

## 51.2 最难的问题：实时与历史无缝拼接

轨迹页面有个棘手的情况：run 可能是**已经跑完的**（从历史接口一次性拉全部）、**正在跑的**（用 SSE 实时接收）、或者**看着看着就跑完了**（前一刻还在实时流、后一刻变历史）。

解法：把所有状态封进一个自定义 Hook，组件完全无感知：

```typescript
function useTrace(runId: string) {
  const { data: run } = useRun(runId);
  const isLive = run?.status === "running";

  // 历史部分：分页拉取（跑完的拉全部，在跑的拉到当前）
  const pages = useInfiniteQuery({ queryKey: ["trace", runId], queryFn: ... });

  // 实时部分：从历史部分的末尾"接着"用 SSE 续上
  const liveEvents = useRunStream(isLive ? runId : null, lastSeqOf(pages.data));

  // 两段合并、去重（靠 seq 序号精确对齐）
  const events = useMemo(() => mergeBySeq(flatten(pages.data), liveEvents),
                         [pages.data, liveEvents]);
  return { run, turns: buildTurns(events), isLive };
}
```

关键是每个事件带**单调递增的 `seq` 序号**（后端分配）。SSE 掉帧时后端发 `lagged` 信号，前端立刻用 REST `/trace?offset=<lastSeq>` 补全历史——这个设计是在后端事件模型阶段就为前端预留好的，体现全栈视角。

## 51.3 仪表盘：趋势图 + 实时统计卡

仪表盘（`features/dashboard/DashboardPage.tsx`）用 recharts 绘制趋势图，数据来自 `/api/stats/trend`：

```tsx
<ResponsiveContainer width="100%" height={260}>
  <LineChart data={trendData}>
    <CartesianGrid strokeDasharray="3 3" />
    <XAxis dataKey="time" />
    {/* 左轴：通过率（0-100%）*/}
    <YAxis yAxisId="rate" domain={[0, 100]} tickFormatter={v => `${v}%`} />
    {/* 右轴：Run 数量（绝对值）*/}
    <YAxis yAxisId="count" orientation="right" />
    <Tooltip />
    <Line yAxisId="rate" dataKey="pass_rate" name="通过率" stroke="#6366f1" />
    <Line yAxisId="count" dataKey="total"    name="Run 数"  stroke="#10b981" strokeDasharray="4 2" />
  </LineChart>
</ResponsiveContainer>
```

双 Y 轴设计：通过率（0-100%）和 Run 数量用不同量纲，必须分开，否则一条线会被另一条"压扁"。

四个统计卡片实时刷新（每 15 秒 refetch），边框颜色随通过率变化：

```tsx
<StatCard
  label="整体通过率"
  value={`${passRatePct}%`}
  color={dash.pass_rate >= 0.8 ? "#10b981" : dash.pass_rate >= 0.5 ? "#f59e0b" : "#ef4444"}
/>
```

这个"绿/黄/红"阈值是生产里常见的 SLO 可视化模式——80% 以上绿，50%~80% 橙，50% 以下红，一眼知道是否达标。

## 51.4 对比页面：URL 驱动 + verdict 过滤

对比页面（`features/compare/ComparePage.tsx`）完全由 URL 参数驱动：

```tsx
// /compare?a=<batch_uuid>&b=<batch_uuid>
const [params] = useSearchParams();
const a = params.get("a") ?? "";
const b = params.get("b") ?? "";

const { data } = useQuery({
  queryKey: ["compare", a, b],
  queryFn: () => fetchCompare(a, b),
  enabled: Boolean(a && b),
});
```

URL 驱动的好处是链接可分享——内部工具的硬需求，写完对比直接把 URL 发给同事，打开就是一样的视图。

三种 verdict 用不同颜色区分，加过滤器按钮：

```tsx
const VERDICT_COLOR = {
  regression:  "#ef4444",  // 红：退步，最重要
  improvement: "#10b981",  // 绿：进步
  same:        "#9ca3af",  // 灰：无变化
};
```

**可比性警告**：后端发现两个批次的模型/镜像不同时会在响应里带 `comparability_warnings`，前端用橙色提示框显著展示：

```tsx
{comparability_warnings.length > 0 && (
  <div className="warning-box">
    <strong>⚠ 可比性提示</strong>
    <ul>{comparability_warnings.map(w => <li key={w}>{w}</li>)}</ul>
  </div>
)}
```

这一点呼应第 49 章的设计原则：可比性检查在后端做（前端不重复），但**警告展示的责任在前端**。

## 51.5 路由与导航

四个主要路由：

```tsx
// App.tsx
const router = createBrowserRouter([{
  element: <Layout />,
  children: [
    { path: "/",           element: <BatchesPage /> },     // 批次列表
    { path: "/batches/:id", element: <BatchDetailPage /> }, // 批次详情
    { path: "/runs/:id",   element: <TracePage /> },        // 轨迹查看器
    { path: "/dashboard",  element: <DashboardPage /> },    // 仪表盘
    { path: "/compare",    element: <ComparePage /> },      // 对比报告
  ],
}]);
```

顶部导航用 `NavLink` 的 `isActive` 自动高亮当前页——细节，但用户体验差距就在这些细节里。

## 51.6 包管理与依赖

新增 `recharts` 用于趋势图：

```json
// package.json
"dependencies": {
  "@tanstack/react-query": "^5",
  "react-router-dom": "^6",
  "recharts": "^2.13",
  "zod": "^3"
}
```

**为什么选 recharts 而不是 D3**：recharts 是 React 的 declarative wrapper，声明式写法和 React 组件模型吻合，适合组件化的评测平台仪表盘。D3 更灵活但需要命令式 DOM 操作，在 React 里 awkward。如果将来需要复杂的自定义可视化，再迁移到 D3 / Observable Plot。

## 51.7 小结与练习

- 三端类型一致靠 Zod 在运行时验证 + 未知事件降级占位，而不是崩溃。
- 实时 + 历史拼接靠 `seq` 序号对齐，SSE 掉帧后用 REST 补全——这个设计在后端事件模型阶段就要想好。
- 仪表盘双 Y 轴趋势图 + 颜色阈值卡片，一眼看出平台健康状态。
- 对比页面用 URL 参数驱动，链接可分享，verdict 过滤和可比性警告是核心用户价值。

**练习**

1. 给仪表盘加一张"每种 scaffold 的平均成本"柱状图，数据从 `/api/stats/dashboard` 的 `status_breakdown` 扩展。
2. 对比页面加"下载 CSV"功能——把 cases 列表导出，供数据分析师用 Excel 进一步处理。
3. 轨迹查看器加"快速搜索"：按工具名过滤只看某类工具调用（比如只看 `bash`），实现纯前端过滤，不需要额外后端接口。

> **下一章**：部署上线——MinIO + Prometheus + Grafana 的完整 docker-compose，K8s 生产配置，以及让整个项目真正跑起来的最后一步。
