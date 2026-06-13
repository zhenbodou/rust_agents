# 第 34 章 TypeScript：给 JavaScript 装上"安全带"

> 你已经会写 JavaScript 了。但 JS 有个大问题：它太"宽容"，很多错误要等到上线运行才暴露。TypeScript（简称 TS）就是来解决这个的——它给 JS 加上一层"类型检查"，让你在**写代码的当下**就被编辑器划红线提醒哪里写错了。所有 JS 代码都是合法的 TS，TS 只是在上面加了一层保护。本章从第一个类型标注，讲到搭起一个正经前端工程。

## 34.1 为什么要多学一门 TypeScript

先看一个 JS 的真实事故，体会问题在哪：

```javascript
function totalMs(events) {
  return events.reduce((sum, e) => sum + e.durationMs, 0);
}
totalMs([{ duration_ms: 320 }]);   // 结果是 NaN（不是数字）！
```

发现 bug 了吗？数据里的字段叫 `duration_ms`（下划线），但代码里写的是 `e.durationMs`（驼峰）。JS **不会报错**，它默默地把不存在的字段当成 `undefined`，`undefined + 0` 就成了 `NaN`。这种 bug 要等到用户用的时候才发现，排查还特别费劲。

TS 版本会在你**敲下这行的瞬间**就在编辑器里划红线：`属性 durationMs 不存在`。项目越大、数据结构越复杂，这层保护越值钱。而 Agent 前端恰恰是数据复杂度拉满的场景：

- **轨迹查看器**：要渲染成千上万条工具调用、token 流、diff，事件有十几种不同形状；
- **评测仪表盘**：通过率、成本、版本对比，数据字段一大堆；

这些界面可以做得朴素，但**数据量大、类型必须严谨**。所以前端工程界的共识是：稍微正经一点的项目，都用 TypeScript。

> **写过 Rust 的读者**：可以把 TS 类比成"带 GC 的、可选的类型系统"——`interface` 像 `struct`，判别联合像 `enum`，类型收窄像 `match`。但 TS 的类型是"可选且可被绕过的"（有个逃生舱叫 `any`），纪律得靠你把配置调严来兜底。没学过 Rust 的读者忽略这段即可，下面从零讲。

## 34.2 第一步：给值标上"类型"

类型，就是"这个值是什么种类"的标注。最快的上手方式是打开 [typescriptlang.org/play](https://www.typescriptlang.org/play)（在线即开即用，不用装任何东西）。基础语法只有一个：**在值后面加 `: 类型`**。

```typescript
// 给变量标类型（其实大多时候可以省略，TS 会自己推断）
const name: string = "mcc";       // 字符串
let count: number = 0;            // 数字
let ok: boolean = true;           // 布尔
const tags: string[] = ["a", "b"]; // 字符串数组

// 给函数标类型（参数必须标，返回值通常能自动推断）
function totalMs(events: TraceEvent[]): number {
  return events.reduce((sum, e) => sum + e.durationMs, 0);
}
```

**描述一个对象的"形状"**，用 `interface`（接口）：

```typescript
interface ToolCall {
  toolName: string;
  durationMs: number;
  isError?: boolean;       // 名字后面加 ? 表示"这个字段可有可无"
  readonly id: string;     // readonly 表示"只读，不许改"
}
```

有了它，谁要传一个缺字段或字段类型不对的对象，TS 立刻报错。

**联合类型**：用 `|` 表示"只能是这几个值之一"，特别适合状态这种有限选项：

```typescript
type Status = "queued" | "running" | "passed" | "failed";
let s: Status = "running";
// s = "done";   // ✗ 报错！"done" 不在允许的集合里 —— 拼写错误从此无处遁形
```

**关键心法**：TS 的类型描述的是"值长什么样"。你在第 32 章已经会设计数据了（对象怎么嵌套、数组装什么），TS 只是让你把脑子里的设计**写下来**，然后由编辑器时刻帮你核对有没有用错。它不是新负担，是把你本来就该想清楚的东西变得显式。

很多时候你**不用手写类型**，TS 能自己推断：

```typescript
const events = [{ toolName: "bash", durationMs: 320 }];
// TS 自动推断出 events 是"对象数组"，每个对象有 toolName 和 durationMs
// 原则：能让 TS 自己推断的就别手写，保持代码干净
```

## 34.3 第二步：搭起工程环境

光在 playground 玩不够，真实项目要在自己电脑上搭。先装好基础工具（这些是前端工程的"标配工具链"）：

```bash
# 1. 装 Node.js 版本管理器 fnm（管理 Node 版本，类似别的语言的版本管理器）
curl -fsSL https://fnm.vercel.app/install | bash
fnm install 22 && fnm use 22       # 安装并使用 Node 22

# 2. 启用 pnpm（包管理器，负责下载第三方库；比老牌的 npm 更快更省空间）
corepack enable

# 3. 新建一个 TS 项目
mkdir trace-lib && cd trace-lib
pnpm init                          # 生成 package.json（项目的"户口本"）
pnpm add -D typescript vitest      # 装 TS 编译器和测试工具（-D 表示开发时才用）
pnpm tsc --init                    # 生成 TS 配置文件 tsconfig.json
```

`tsconfig.json` 是 TS 的配置文件。新手只要确保里面这一项是打开的，就拿到了 80% 的好处：

```jsonc
{
  "compilerOptions": {
    "strict": true,                    // 总开关：开启所有严格检查（务必打开）
    "noUncheckedIndexedAccess": true   // 访问数组/对象时强制考虑"可能不存在"的情况
  }
}
```

`"strict": true` 是底线。它逼着你处理各种"可能为空"的情况，正是这些检查帮你消灭了大量潜在 bug。

## 34.4 第三步：用类型给 Agent 轨迹建模（核心技能）

直接拿真实场景练。Agent 一次运行会产生一串事件，每种事件形状不同。这正是 TS 最闪光的地方——**判别联合**（用一个字段区分是哪种）：

```typescript
// 一个 TraceEvent 可能是下面几种之一，靠 type 字段区分
export type TraceEvent =
  | { type: "run_started"; runId: string; task: string; ts: number }
  | { type: "llm_chunk"; turn: number; delta: string; ts: number }
  | { type: "tool_call"; turn: number; toolName: string; args: unknown; ts: number }
  | { type: "tool_result"; output: string; isError: boolean; durationMs: number; ts: number }
  | { type: "run_finished"; status: "success" | "failure"; costUsd: number; ts: number };
```

魔法在于：当你用 `switch` 按 `type` 分情况处理时，TS 会**自动收窄**类型——在每个分支里，它确切知道这个事件是哪种，于是你访问字段时有精准的提示和检查：

```typescript
export function describe(event: TraceEvent): string {
  switch (event.type) {
    case "tool_call":
      // 在这个分支里，TS 知道 event 一定是 tool_call，所以 event.toolName 合法
      return `→ 调用 ${event.toolName}`;
    case "tool_result":
      // 这里 TS 知道有 isError 和 durationMs
      return event.isError ? `✗ 失败` : `✓ ${event.durationMs}ms`;
    case "run_finished":
      return `结束：${event.status}，花了 $${event.costUsd.toFixed(4)}`;
    default:
      return event.type;
  }
}
```

如果你在 `tool_call` 分支里手滑写了 `event.costUsd`（那是 `run_finished` 才有的字段），TS 立刻报错。**这就是类型系统替你挡住 bug 的样子**——你几乎不可能再把不同事件的字段搞混。

## 34.5 第四步：在"数据入口"做运行时校验

这里有个重要的认知。TS 的类型检查**只在你写代码时有效，程序运行时类型就不存在了**（编译后变回普通 JS）。所以当数据从外部进来（比如后端返回的 JSON），TS 管不着——`JSON.parse` 出来的东西，TS 只能当它是"任意类型"，这是事故之源。

解决办法：在"数据入口"做一次**运行时校验**。前端最常用的工具叫 Zod——你用它写一份"数据该长什么样"的规则，它既能在运行时检查真实数据，又能自动推导出 TS 类型：

```typescript
import { z } from "zod";

// 写一份"工具调用事件"的校验规则
const ToolCallSchema = z.object({
  type: z.literal("tool_call"),
  turn: z.number().int().nonnegative(),    // 必须是非负整数
  toolName: z.string().min(1),             // 必须是非空字符串
  ts: z.number(),
});

// 从校验规则反向得到 TS 类型，不用再手写 interface（单一来源，不会对不上）
type ToolCall = z.infer<typeof ToolCallSchema>;

// 在收到数据时校验
function parseEvent(raw: string): ToolCall {
  return ToolCallSchema.parse(JSON.parse(raw));   // 数据不合规会抛出带详细路径的错误
}
```

**心智模型**：把 `unknown`（未知/未验证的数据）这个类型当成"未拆封的快递"——TS 不让你直接用，必须先用 Zod 拆封验明正身，才能安全使用。规则：所有外部数据进来时用 Zod 校验，程序内部禁用逃生舱 `any`。

## 34.6 异步与类型

第 33 章学的 `async/await`、`fetch`，配上类型会更安全。一个 Agent 前端常用的模式——并发拉取多个 run 并校验：

```typescript
async function fetchRuns(runIds: string[]): Promise<Map<string, TraceEvent[]>> {
  const results = await Promise.all(
    runIds.map(async (id) => {
      const res = await fetch(`/api/runs/${id}/events`);
      if (!res.ok) throw new Error(`HTTP ${res.status}`);   // 别忘了第 33 章的坑
      const json: unknown = await res.json();               // 进来是"未验证"的
      return [id, z.array(TraceEventSchema).parse(json)] as const;  // 校验后才放心用
    }),
  );
  return new Map(results);
}
```

注意 `json: unknown`——这是好习惯，强迫你在用之前先校验。

## 34.7 测试与质量门禁

正经项目要有自动化检查。用 Vitest 写测试（写法和你将来跑测试的命令）：

```typescript
// trace.test.ts
import { describe, it, expect } from "vitest";
import { parseEvent } from "./trace";

describe("parseEvent", () => {
  it("拒绝缺字段的畸形数据", () => {
    expect(() => parseEvent(`{"type":"tool_call"}`)).toThrow();   // 缺字段，应该抛错
  });
});
```

提交代码前跑这几道关卡（第 48 章会把它们放进 CI 自动执行）：

```bash
pnpm tsc --noEmit     # 类型检查（只检查不生成文件）
pnpm vitest run       # 跑测试
```

## 34.8 小结与练习

- TS = JS + 类型检查，在你写错的当下就划红线，项目越大越值钱。
- 核心武器：`interface` 描述对象形状、联合类型 `|` 限定取值、**判别联合 + switch 自动收窄**给多形态数据建模。
- `"strict": true` 是底线；能推断就别手写类型；逃生舱 `any` 尽量不用。
- 类型只在写代码时有效，**外部数据进来必须用 Zod 做运行时校验**；把 `unknown` 当未拆封快递对待。

**练习**

1. 给第 32 章的轨迹页面数据写一套完整的 `TraceEvent` 判别联合类型，覆盖所有事件种类，并写一个 `describe(event)` 函数，确认每个分支里字段访问都有正确提示。
2. 为 `mini-claude-code` 的 session 文件（JSONL，每行一个 JSON）写一套 Zod 校验规则，再写一个小脚本读取并统计每种工具的调用次数和平均耗时。
3. 故意往数据里塞一个字段类型不对的事件，确认 Zod 校验会报错并指出是哪个字段错了。
4. 把 `"strict"` 关掉再打开，观察编辑器里多出来的红线，体会严格模式到底帮你挡了哪些坑。

> **下一章**：React 入门——前端真正的主角登场。我们从"写第一个组件"开始，一步步把第 32 章的轨迹页面用 React 重做一遍，你会看到它如何把"状态变了手动重画"自动化。
