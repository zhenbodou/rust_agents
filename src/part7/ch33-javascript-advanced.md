# 第 33 章 JavaScript 进阶：闭包、异步与事件循环

> 上一章的 JS 够你写出能交互的页面了。本章这四个主题——闭包、this、异步、事件循环——是从"会写"到"真懂"的分水岭，也是 JS 面试的"四大名著"。它们听起来吓人，但每一个都对应你写真实应用时绕不开的问题。我们照旧从生活类比讲起，不让你死记硬背。

## 33.1 作用域与闭包

### 33.1.1 先理解作用域：变量"能被谁看见"

"作用域"就是一个变量的"可见范围"。规则很符合直觉：**里面能看见外面，外面看不见里面。**

```javascript
const global = "全局，谁都能看见";

function outer() {
  const a = "只在 outer 里能看见";
  function inner() {
    const b = "只在 inner 里能看见";
    console.log(global, a, b);   // ✓ 里层能看见外层所有变量
  }
  inner();
  // console.log(b);   // ✗ 报错！外层看不见内层的 b
}
```

打个比方：你站在屋里（内层）能看见窗外的院子和街道（外层），但站在街上（外层）看不见别人家屋里（内层）有什么。这种"从里往外一层层找变量"的机制叫**作用域链**。

注意一个细节：上一章说"别用 `var`"，原因就在这——`let`/`const` 声明的变量被 `{}` 大括号限制住（块级作用域），而 `var` 无视大括号，容易漏到外面造成 bug。

### 33.1.2 闭包：函数会"记住"它出生的地方

闭包是 JS 里最重要、也最让新手困惑的概念。先给一句话定义，再慢慢解释：

> **闭包 = 一个函数 + 它出生时所在的那个作用域。** 函数被带到哪里执行，都还记得出生地的变量。

用例子感受。下面这个"计数器工厂"，每喊一次就让数字加一：

```javascript
function makeCounter() {
  let count = 0;              // 这个 count 本该在函数结束时消失……
  return () => {             // ……但返回的这个小函数"记住"了它
    count += 1;
    return count;
  };
}

const counter = makeCounter();
counter();   // 1
counter();   // 2     ← count 没有消失！它被返回的函数"包"住了，这就是"闭包"
counter();   // 3

const another = makeCounter();
another();   // 1     ← 每次调用 makeCounter 都产生一个全新的、独立的 count
```

神奇的地方在于：`makeCounter` 早就执行结束了，按理说它内部的 `count` 应该被清理掉。但返回的那个小函数还需要用 `count`，于是 JS 就把 `count` 一直留着——**函数"包住"了它需要的外部变量，这个"包"就叫闭包**。

**为什么你必须懂它？** 因为上一章天天用的"回调"全靠闭包工作：

```javascript
function subscribeRun(runId) {
  // 这个回调可能一小时后才被触发，但它依然记得 runId 是多少 —— 闭包的功劳
  socket.addEventListener("message", (e) => {
    console.log(`run ${runId} 收到消息`, e.data);
  });
}
```

闭包还有两个前端每天都在用的实用模式（第 37 章流式渲染会用到 throttle）：

```javascript
// 防抖 debounce：等"风暴"平息后才做事
// 典型场景：搜索框，用户停止打字 300 毫秒后才真正发请求，避免每敲一个字就请求一次
function debounce(fn, ms) {
  let timer;                                   // 这个 timer 被闭包私藏着
  return (...args) => {
    clearTimeout(timer);                       // 来新的就取消上一个计划
    timer = setTimeout(() => fn(...args), ms); // 重新计时
  };
}
search.addEventListener("input", debounce(render, 300));

// 节流 throttle：风暴期间，按固定频率做事
// 典型场景：滚动、流式 token，每 16 毫秒最多渲染一次，不被高频事件淹没
function throttle(fn, ms) {
  let last = 0;
  return (...args) => {
    const now = Date.now();
    if (now - last >= ms) { last = now; fn(...args); }
  };
}
```

`(...args)` 是"把所有参数收集起来原样转交"的意思，不用纠结，照着用即可。

### 33.1.3 一个经典陷阱（面试高频）

闭包有个著名陷阱，看懂它你就真懂闭包了：

```javascript
// 用 var（错误示范）
for (var i = 0; i < 3; i++) {
  setTimeout(() => console.log(i), 100);
}
// 输出 3 3 3 ！
// 因为 var 没有块级作用域，三个回调"包"住的是同一个 i；
// 等 100 毫秒后回调真正执行时，循环早跑完了，i 已经是 3

// 用 let（正确）
for (let i = 0; i < 3; i++) {
  setTimeout(() => console.log(i), 100);
}
// 输出 0 1 2
// let 让每一轮循环都有一个"全新的 i"，三个回调各自包住各自的那个
```

这又是一个"用 let 不用 var"的理由。第 36 章学 React 时会撞上这个陷阱的现代版本（叫 stale closure，过期闭包）——现在懂了原理，到时候就不踩坑。

## 33.2 this 与类：先够用，别钻牛角尖

`this` 是 JS 公认最迷惑的点。好消息是：写现代前端（尤其 React 函数组件），你很少需要和 `this` 死磕。这里给你"够用"的理解，不深挖。

**一句话规则：`this` 是什么，取决于函数"怎么被调用"，而不是"在哪里定义"。**

```javascript
const run = {
  id: "run-42",
  describe() { return `Run ${this.id}`; },
};

run.describe();        // "Run run-42"  —— 通过 run.方法() 调用，this 就是点号前的 run

const fn = run.describe;
fn();                  // 报错！—— 把方法"拆下来"单独调用，this 丢了
```

最后一行是新手高频 bug："方法一旦从对象上拆下来单独调用，就丢了 this"。

**实用结论：在回调里优先用箭头函数。** 因为箭头函数没有自己的 `this`，它直接用外层的，正好避开上面的坑：

```javascript
class Poller {
  constructor() { this.count = 0; }
  start() {
    setInterval(() => {
      this.count++;        // ✓ 箭头函数，this 还是这个 Poller 实例
    }, 1000);
  }
}
```

至于 `class`（类）和它背后的"原型链"机制，你现在只需要知道：**class 是用来创建"有状态的对象"的模板**。写一个有内部状态的解析器时会用到：

```javascript
class TraceParser {
  #buffer = "";                    // # 开头表示"私有"，外部碰不到

  constructor(onEvent) {
    this.onEvent = onEvent;        // 收到完整一行时调用的回调
  }

  feed(chunk) {                    // 喂给它一段文本
    this.#buffer += chunk;
    const lines = this.#buffer.split("\n");
    this.#buffer = lines.pop() ?? "";   // 最后一段可能不完整，留着等下次
    for (const line of lines) {
      if (line.trim()) this.onEvent(JSON.parse(line));
    }
  }
}
```

> **现代前端的基调**：业务代码里"函数 + 组合"用得远比"类 + 继承"多（React 已全面转向函数组件）。所以 `this` 和 `class` 你**够用就行**，不必现在啃透。但读框架源码时会遇到，知道上面这些足够应付。

## 33.3 异步编程：JS 怎么"一边等一边干别的"

这是本章最实用的部分，请放慢看。

### 33.3.1 为什么需要异步

JS 在浏览器里是**单线程**的——同一时刻只能做一件事，而且这条线程还兼职画界面。想象一个只有一名服务员的餐厅：如果他给一桌点完菜后，**站在厨房门口干等这桌的菜做好**，整个餐厅就瘫痪了。

正确做法是：服务员点完菜（发起请求）马上去服务别桌（继续干活），菜好了厨房喊一声（结果回来了通知他），他再去端。这就是**异步**——发起一个耗时操作后不傻等，等它好了再回来处理。网络请求、读文件、定时器，在 JS 里全是异步的。

### 33.3.2 异步写法的三代进化

"等好了通知我"这件事，JS 历史上有三种写法，一代比一代好读：

```javascript
// 第一代：回调。一层套一层，俗称"回调地狱"，没法看
fetchRun(id, (run) => {
  fetchTrace(run.traceUrl, (trace) => {
    parseEvents(trace, (events) => {
      render(events);
    });
  });
});

// 第二代：Promise。用 .then() 串成链，平了
fetchRun(id)
  .then((run) => fetchTrace(run.traceUrl))
  .then((trace) => parseEvents(trace))
  .then((events) => render(events))
  .catch((err) => showError(err));     // 整条链的错误，一处接住

// 第三代：async/await。写起来几乎和同步代码一样直白（强烈推荐）
async function load(id) {
  try {
    const run = await fetchRun(id);        // await = "等这个做完，再往下走"
    const trace = await fetchTrace(run.traceUrl);
    render(parseEvents(trace));
  } catch (err) {
    showError(err);                        // 任何一步出错都到这
  }
}
```

记住两个关键词：**`async`** 加在函数前面，表示"这是个异步函数"；**`await`** 加在一个耗时操作前面，表示"等它出结果再继续"。`await` 看起来像在"等"，但它不会冻结页面——它只是把"后续代码"挂起，让出线程去干别的，结果好了再回来。这就是单线程却不卡的奥秘。

### 33.3.3 一个新手必踩的性能坑：该并发时别串行

如果两件事互不依赖，**别一个一个 await**，那样会白白慢一倍：

```javascript
// ✗ 慢：先等 A 好，再开始 B（两段时间相加）
const runA = await fetchRun(a);
const runB = await fetchRun(b);

// ✓ 快：两个同时发起，一起等（取较长的那个时间）
const [runA, runB] = await Promise.all([fetchRun(a), fetchRun(b)]);
```

`Promise.all([...])` 是"把这几件事同时启动，全部完成后一起拿结果"。相关的还有几个，用到再查：`Promise.allSettled`（不怕个别失败，每个都给你结果）、`Promise.race`（谁先好用谁，可用来做超时）。

### 33.3.4 fetch：真正发一个网络请求

`fetch` 是浏览器内置的发请求工具。有一个**必须记住的坑**：

```javascript
async function fetchRuns() {
  const res = await fetch("/api/runs", {
    headers: { Authorization: `Bearer ${token}` },
  });
  // 坑：服务器返回 404 或 500 时，fetch 不会报错！只有断网才报错。
  // 所以必须自己检查 res.ok：
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.json();         // 把响应体解析成对象（这一步也是异步的）
}

// 发 POST（提交数据）
await fetch("/api/batches", {
  method: "POST",
  headers: { "Content-Type": "application/json" },
  body: JSON.stringify({ suiteId, profileId }),   // 对象要转成 JSON 文本
});
```

### 33.3.5 逐块读取流（Agent 前端的招牌场景）

Agent 的输出是"一个字一个字蹦出来"的流，前端要边收边显示。这用到"异步迭代器"（一种可以一边等一边逐个产出的东西，第 37 章 SSE 的底层）：

```javascript
// async function* 是"异步生成器"，能一边 await 一边逐个 yield
async function* readLines(response) {
  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let buf = "";
  while (true) {
    const { done, value } = await reader.read();   // 等下一块数据
    if (done) break;
    buf += decoder.decode(value, { stream: true });
    const lines = buf.split("\n");
    buf = lines.pop() ?? "";       // 最后一段可能不完整，留着
    yield* lines;                  // 把完整的行逐个产出
  }
}

// for await 专门用来消费异步迭代器
const res = await fetch("/api/runs/42/stream");
for await (const line of readLines(res)) {
  appendEvent(JSON.parse(line));   // 数据每到一块就渲染一块
}
```

这段现在看不全懂没关系，第 37 章会专门讲流式 UI，到时回头看就通了。

## 33.4 事件循环：单线程为什么不卡（面试终极题）

"讲讲 event loop"几乎是每场 JS 面试的压轴题。其实模型很简单，用"服务员"类比就能讲清。

把浏览器想象成那个单线程服务员，他面前有两个待办清单：

```
正在手头做的事（调用栈）
      │ 手头活干完后，先把"插队清单"全清空，再从"排队清单"取一件
┌─────▼──────────────────────────────────┐
│ 插队清单（微任务）：Promise 的后续、await 之后的代码 │  ← 优先级高，一次全做完
├──────────────────────────────────────────┤
│ 排队清单（宏任务）：setTimeout、各种事件回调       │  ← 一轮只取一件
└──────────────────────────────────────────┘
```

规则就三句话：**先把手头的同步代码一口气做完 → 然后把"插队清单"（微任务）全部做完 → 再从"排队清单"（宏任务）取一件做**，如此循环。

用这道经典题验证你懂了：

```javascript
console.log("1");                                // 同步，立刻做
setTimeout(() => console.log("2"), 0);           // 宏任务，进排队清单
Promise.resolve().then(() => console.log("3"));  // 微任务，进插队清单
console.log("4");                                // 同步，立刻做

// 输出顺序：1 4 3 2
// 先做完同步的 1 和 4 → 再清插队清单里的 3 → 最后才轮到排队清单里的 2
// （注意：哪怕 setTimeout 写的是 0 毫秒，它也得乖乖排队，排在微任务后面）
```

**这对你写代码有两个实际影响：**

1. **一段超长的同步代码会冻结页面**——服务员被一件大事缠住，没空画界面。比如解析一个 50MB 的轨迹文件，要么切成小块分批做，要么扔给"Web Worker"（一个独立线程）去算。
2. **`await` 不冻结页面**——它是"把后续代码登记为待办，先让位"，所以 await 等待期间界面照样能响应你的点击。

## 33.5 模块：把代码拆成多个文件

上一章我们所有代码都写在一个文件里，项目一大就乱。模块系统让你把代码拆开，再按需"导出/导入"：

```javascript
// trace-parser.js —— 在这个文件里导出东西
export function parseEvent(line) { /* ... */ }
export const SCHEMA_VERSION = 3;

// app.js —— 在另一个文件里导入用
import { parseEvent, SCHEMA_VERSION } from "./trace-parser.js";
```

在 HTML 里引入模块脚本要加 `type="module"`：

```html
<script type="module" src="app.js"></script>
```

`import`/`export` 是现代标准写法（叫 ESM）。偶尔在老代码里看到 `require()` 是上一代写法（CommonJS），认识即可。模块的好处：每个文件是独立小天地，不会有全局变量互相污染——上一章"全写在一个文件"的时代到此结束。

## 33.6 错误处理与调试

异步代码也用 `try-catch` 接错误（`await` 出的错会被 catch 接住）：

```javascript
try {
  await loadRun(id);
} catch (e) {
  showError(e);        // 任何一步失败都到这
} finally {
  hideSpinner();       // 不管成功失败，最后都执行（比如关掉转圈动画）
}
```

最后教你一个比 `console.log` 高效得多的调试武器——**断点**：打开 DevTools 的 Sources 面板，点某一行的行号，刷新页面，代码会停在那一行，你可以悬停看每个变量的值、一行行单步执行。或者直接在代码里写一句 `debugger;`，运行到那也会停下。学会断点调试，排查 bug 的效率会高一个量级。

## 33.7 小结与练习

- 闭包 = 函数 + 它出生的作用域；回调能记住外部变量全靠它；debounce/throttle 是它的日常用法；用 `let` 避开循环陷阱。
- `this` 看"怎么调用"，回调里优先用箭头函数；`class` 够用就行，不必现在钻透。
- 异步三代：回调 → Promise → async/await（推荐）；该并发时用 `Promise.all` 别串行；`fetch` 要自查 `res.ok`；流式数据用异步迭代器 + `for await`。
- 事件循环：同步 → 微任务 → 宏任务；长同步任务会冻结页面，`await` 不会。

**练习**

1. 手写 `debounce` 和 `throttle`，并接到第 32 章的搜索框上，体会两者行为差异。
2. 不查资料，先猜下面代码的输出，再到 Console 验证：
   ```javascript
   console.log("c");
   Promise.resolve().then(() => console.log("f"));
   setTimeout(() => console.log("e"));
   console.log("d");
   ```
3. 用异步生成器写一个 `mockEventStream(events, ms)`：每隔 `ms` 毫秒逐个产出一个事件，用 `for await` 消费。这是第 37 章测试流式 UI 的小道具，写完留着。
4. 把第 32 章的轨迹页面拆成 `data.js`、`render.js`、`app.js` 三个模块，数据改成用 `fetch("./events.json")` 异步加载，并加上"加载中"和"加载失败"两种状态显示。

> **下一章**：TypeScript——给 JavaScript 加上"类型检查"，让编辑器在你写错时当场提醒，是搭建正经前端工程的必备。
