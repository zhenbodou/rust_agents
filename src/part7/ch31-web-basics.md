# 第 31 章 Web 与前端基础：浏览器、HTML、CSS 从零开始

> 从这一章起，我们进入前端世界。我**假设你从未写过一行网页代码**，连"什么是浏览器在干活"都没想过——完全没关系。这一章我们一步一步来，每个新词第一次出现都会解释清楚。读完你会做出一个真正能在浏览器里打开的网页，并理解它背后发生的事。我们的最终目标是做出一个"Agent 轨迹查看器"（看 Agent 一步步干了什么的界面），但今天先从最最基础的开始。

## 31.1 先搞清楚：你每天用的浏览器到底在做什么

你每天都在用浏览器（Chrome、Edge、Safari），但可能从没想过：当你输入一个网址、按下回车，到屏幕上出现花花绿绿的页面，中间发生了什么？

我们用一个生活类比。**点外卖**的过程是这样的：

1. 你在 App 里点了一家店（输入网址）；
2. App 找到这家店的地址（把网址翻译成服务器的位置）；
3. App 把你的订单发过去（发送请求）；
4. 餐厅做好饭，打包发回来（服务器返回内容）；
5. 外卖到了，你把它摆上桌、开吃（浏览器把内容画成页面）。

浏览器打开网页，几乎就是这个流程。把专业词填进去：

```
1. DNS 解析     把 example.com 翻译成一串数字地址（IP），就像把店名翻译成门牌号
2. 建立连接     浏览器和服务器"接上头"（https 的 s 表示这条线是加密的，别人偷看不到）
3. 发送请求     浏览器发一段文字过去："我要看首页"（这段文字叫 HTTP 请求）
4. 服务器响应   服务器回一段文字：内容 + 一个"状态码"（比如 200 表示成功）
5. 解析渲染     浏览器把收到的内容画成你看到的页面 ← 本章重点
```

你现在只需要记住一件事：**网页本质上就是服务器发给你的一堆文本，浏览器负责把这堆文本"画"出来。** 这堆文本主要分三种，它们是前端世界的"三巨头"。

## 31.2 前端三巨头：各管一摊

盖一栋房子需要三种角色：搭骨架的、做装修的、装水电的。网页也一样，由三种技术分工合作：

| 技术 | 管什么 | 盖房子类比 | 这本书第几章学 |
|---|---|---|---|
| **HTML** | 内容和结构（页面上**有什么**） | 房子的骨架、墙、房间 | 本章 |
| **CSS** | 外观和布局（**长什么样**） | 刷漆、铺地板、摆家具 | 本章 |
| **JavaScript** | 行为和交互（**能做什么**） | 通水电、装智能开关 | 第 32–33 章 |

只有 HTML，你会得到一个能看但很丑、没法互动的页面（像毛坯房）。加上 CSS，它变好看了（精装修）。再加上 JavaScript，它能响应你的点击、能动起来（智能家居）。

本章我们专注前两个：HTML（搭骨架）和 CSS（做装修）。学完它们，你就能做出好看的静态页面了。

## 31.3 准备工具：只要两样东西

学前端不需要装一堆复杂软件。两样就够：

**第一样：浏览器（推荐 Chrome）**。它不只是用来上网的——它内置了一套强大的"开发者工具"。现在就试一下：随便打开一个网页，按键盘的 `F12`（Mac 上是 `Cmd+Option+I`，或者右键点页面选"检查"）。弹出来的这个面板就是**开发者工具（DevTools）**，前端工程师每天都泡在里面。先认识三个标签页，后面会反复用：

- **Elements（元素）**：能看到这个网页的"骨架"，还能当场改它试试效果；
- **Console（控制台）**：一个能直接运行代码的小窗口，下一章你会天天用；
- **Network（网络）**：能看到刚才 31.1 说的那些"请求"真实地发生。

**第二样：一个写代码的编辑器（推荐 VS Code）**。去 [code.visualstudio.com](https://code.visualstudio.com) 免费下载安装。装好后，建议再装一个叫 **Live Server** 的插件（在 VS Code 左侧的扩展商店里搜名字、点安装）——它能让你一改代码、浏览器就自动刷新，非常省事。

准备好后，建一个文件夹放我们的练习：

```bash
mkdir my-first-page      # 新建一个文件夹
cd my-first-page         # 进去
code .                   # 用 VS Code 打开这个文件夹（. 表示"当前文件夹"）
```

## 31.4 HTML：写出你的第一个网页

在 VS Code 里新建一个文件，命名为 `index.html`（网页的"首页"约定俗成叫这个名字），把下面的内容敲进去。**不要复制粘贴，自己敲一遍**——手敲能让你记住语法。每行后面的注释解释它干嘛：

```html
<!DOCTYPE html>                    <!-- 告诉浏览器：这是一个 HTML5 网页 -->
<html lang="zh-CN">                <!-- 整个网页的最外层"盒子"，lang 说明是中文 -->
<head>                             <!-- 头部：放给浏览器看的信息，不显示在页面上 -->
  <meta charset="UTF-8">           <!-- 用 UTF-8 编码，不写的话中文会变乱码 -->
  <title>我的第一个网页</title>     <!-- 浏览器标签页上显示的标题 -->
</head>
<body>                             <!-- 身体：用户真正看到的所有东西都放这里 -->
  <h1>你好，前端世界</h1>          <!-- 一个大标题 -->
  <p>这是我写的第一段文字。</p>     <!-- 一个段落 -->
</body>
</html>
```

保存（`Ctrl+S` / `Cmd+S`），然后右键点这个文件选 "Open with Live Server"。浏览器会自动打开，你看到了自己写的页面——**恭喜，你已经是前端开发者了**。

现在停下来，看懂 HTML 的唯一规则。HTML 全是由"**标签**"组成的，长这样：

```
<标签名>包起来的内容</标签名>
   ↑                  ↑
开始标签           结束标签（多一个斜杠 /）
```

比如 `<h1>你好</h1>` 表示"把'你好'当成一级大标题"。标签可以**套娃**——一个标签里面放别的标签，就像盒子里装小盒子。这种"谁包着谁"的嵌套关系，就构成了网页的骨架树。

> **记住一个核心词：DOM**。浏览器读你的 HTML 时，会在内存里搭出一棵"标签树"，这棵树叫 **DOM（文档对象模型）**。你写的 HTML 标签嵌套关系，就是这棵树的样子。为什么要记它？因为第 32 章的 JavaScript 和第 35 章的 React，本质都是在"修改这棵 DOM 树"——改了树，页面就跟着变。现在不用深究，先把这个词存进脑子。

### 31.4.1 常用标签速成

HTML 标签有很多，但常用的就十几个。把下面这些一个个加进你的 `body` 里，每加一个就保存看效果——**动手看到变化，比读十遍都管用**：

```html
<!-- 标题，h1 最大，h6 最小 -->
<h1>一级标题</h1>
<h2>二级标题</h2>

<!-- 段落和行内强调 -->
<p>普通段落。这里有<strong>加粗的字</strong>和<em>斜体的字</em>。</p>
<p>行内代码长这样：<code>cargo test</code></p>

<!-- 列表 -->
<ul>                            <!-- ul = 无序列表（小圆点） -->
  <li>读取文件 main.rs</li>      <!-- li = 列表中的一项 -->
  <li>运行 cargo test</li>
</ul>
<ol>                            <!-- ol = 有序列表（1.2.3.） -->
  <li>第一步</li>
  <li>第二步</li>
</ol>

<!-- 链接和图片 -->
<a href="https://docs.claude.com">点我去 Claude 文档</a>
<img src="logo.png" alt="图片加载失败时显示这行字">

<!-- 表格：我们的轨迹列表，最早就是长这样 -->
<table>
  <thead>                                      <!-- 表头 -->
    <tr><th>工具</th><th>耗时</th><th>状态</th></tr>   <!-- tr=一行, th=表头格 -->
  </thead>
  <tbody>                                      <!-- 表身 -->
    <tr><td>bash</td><td>320ms</td><td>成功</td></tr>  <!-- td=普通格 -->
    <tr><td>read_file</td><td>12ms</td><td>成功</td></tr>
  </tbody>
</table>
```

还有两个"万能容器"标签，它们本身不显示任何样子，纯粹用来**分组**，方便之后用 CSS 装修：

```html
<div>我是块级容器，会独占一整行</div>
<span>我是行内容器，会乖乖待在文字中间</span>
```

最后是一组"语义化标签"。它们的效果和 `<div>` 一模一样，但名字本身能说明"这块是干嘛的"，让代码更易读、对搜索引擎和盲人读屏软件也更友好（这点第 38a 章会展开）：

```html
<header>页头</header> <nav>导航栏</nav> <main>主要内容</main>
<section>一个区块</section> <aside>侧边栏</aside> <footer>页脚</footer>
```

### 31.4.2 两个关键属性：id 和 class

标签除了名字，还能带"属性"（写在开始标签里的额外信息）。有两个属性后面 CSS 和 JS 都离不开，现在先认识：

```html
<div id="run-42">id 是"身份证"，整个页面里必须唯一，用来精确定位某一个元素</div>
<div class="event-row failed">class 是"分类标签"，可以重复，多个用空格隔开</div>
```

打个比方：`id` 像身份证号（全国唯一），`class` 像"学生""党员"这种身份标签（很多人可以共有）。后面我们会用 `class` 给一类元素统一化妆。

> **动手练习 31-A**：用上面学的标签，写一个"Agent 运行详情"页面：一个大标题、一段任务描述、一个有 5 行的工具调用表格、一个"重新运行"按钮（按钮标签是 `<button>重新运行</button>`）。先别管好不好看，把结构写对就行。

## 31.5 CSS：给网页化妆

现在你的页面能看，但很丑（黑字白底、挤在一起）。CSS（层叠样式表）就是用来美化的。

### 31.5.1 CSS 的写法和引入

一条 CSS 规则长这样——"选中谁 + 怎么打扮"：

```css
h1 {                  /* 选择器：选中所有 <h1> 标签 */
  color: #1f6feb;     /* 属性: 值;  —— 把文字颜色设成蓝色 */
  font-size: 24px;    /* 字号设成 24 像素 */
}
```

`#1f6feb` 是一种写颜色的方式（十六进制，前两位是红、中两位绿、后两位蓝）。你不用背，需要时在 DevTools 里点点就能调。

CSS 放在哪？有三种方式，我们用最规范的第三种——单独存一个 `.css` 文件：

```html
<!-- 方式三：在 HTML 的 <head> 里链接一个外部 CSS 文件（推荐） -->
<link rel="stylesheet" href="style.css">
```

所以新建一个 `style.css` 文件，在 `index.html` 的 `<head>` 里加上上面这行，两个文件就连起来了。

### 31.5.2 选择器：怎么"选中"要化妆的元素

化妆前得先选中对象。最常用的三种选择器：

```css
p            { }   /* 选所有 <p> 标签 */
.event-row   { }   /* 选所有 class="event-row" 的元素（最常用！前面加个点） */
#run-42      { }   /* 选 id="run-42" 的那一个（前面加个井号） */
```

还有一些组合用法，看一眼有印象即可，用到再查：

```css
.timeline .event-row  { }   /* .timeline 里面的 .event-row */
button:hover          { }   /* 鼠标悬停在按钮上时（:hover 叫"伪类"） */
tr:nth-child(odd)     { }   /* 表格的奇数行（用来做斑马纹） */
```

**工程经验**：日常几乎只用 `.class` 选择器，命名清楚就好。这样能避免一个叫"优先级打架"的麻烦（多条规则冲突时谁说了算），新手先记住这条省心原则。

### 31.5.3 盒模型：CSS 最重要的概念

这是 CSS 里你**必须**建立的心智图。在浏览器眼里，**每个元素都是一个盒子**，从里到外有四层：

```
┌───────────── margin（外边距：盒子和邻居之间的距离）─────────────┐
│  ┌────────── border（边框：盒子的边线）──────────┐              │
│  │  ┌─────── padding（内边距：内容和边框的距离）──┐ │              │
│  │  │          content（内容：文字、图片本身）     │ │              │
│  │  └──────────────────────────────────────────┘ │              │
│  └────────────────────────────────────────────────┘              │
└──────────────────────────────────────────────────────────────────┘
```

用具体例子感受：

```css
.event-row {
  padding: 12px 16px;          /* 内边距：上下 12 像素，左右 16 像素 */
  border: 1px solid #d0d7de;   /* 边框：1 像素粗、实线、灰色 */
  border-radius: 6px;          /* 圆角 6 像素 */
  margin-bottom: 8px;          /* 和下面的盒子隔开 8 像素 */
}
```

有一行 CSS 几乎是每个项目的第一行，新手必背，否则盒子尺寸算起来会反直觉：

```css
* { box-sizing: border-box; }   /* * 表示"所有元素"；这行让 width 把 padding 和 border 算进去 */
```

不懂没关系，先抄上。简单说：加了这行，你设 `width: 600px` 就真的是 600 像素宽，不会因为加了 padding 而撑大。

> **想真正搞懂盒模型？** 打开 DevTools 的 Elements 面板，点页面上任意一个元素，右下角会画出这个元素的盒模型四层和具体数值。改改它的值，实时看变化——这是理解盒模型最快的方法。

### 31.5.4 Flexbox：现代布局主力（重点）

"布局"就是决定每个盒子摆在哪、多大。现代前端 70% 的布局靠 **Flexbox**。

最常见的需求：一行里，左边放工具名，右边放耗时，中间自动撑开。Flexbox 三步搞定：

```css
.event-row {
  display: flex;           /* 第一步：告诉这个盒子"你的孩子们横着排" */
  align-items: center;     /* 第二步：孩子们垂直方向居中对齐 */
  gap: 12px;               /* 第三步：孩子之间留 12 像素间隙 */
}
.tool-name { flex: none; } /* 这个孩子：保持自己宽度，不伸缩 */
.summary   { flex: 1; }    /* 这个孩子：尽量长大，占满剩余空间 */
.duration  { flex: none; }
```

配套的 HTML：

```html
<div class="event-row">
  <span class="tool-name">bash</span>
  <span class="summary">cargo test --all</span>
  <span class="duration">320ms</span>
</div>
```

**Flexbox 心智模型**（记住这五个属性就能搞定 90% 布局）：给父盒子写 `display: flex` 让它变成"弹性容器"；`flex-direction` 决定横排(`row`)还是竖排(`column`)；`justify-content` 管主轴方向的对齐；`align-items` 管交叉方向的对齐；孩子用 `flex: 1` 来抢占剩余空间。

用 Flexbox 搭一个三栏布局（这正是第 38 章轨迹查看器的样子：左边列表、中间时间线、右边详情）：

```css
.app {
  display: flex;
  height: 100vh;            /* 100vh = 整个屏幕高度（vh 是"视口高度的 1%"） */
}
.sidebar   { width: 240px; flex: none; overflow-y: auto; }   /* 左栏固定宽 */
.timeline  { flex: 1; overflow-y: auto; }                    /* 中栏弹性、自己滚动 */
.inspector { width: 420px; flex: none; overflow-y: auto; }   /* 右栏固定宽 */
```

`overflow-y: auto` 的意思是"内容太高时这一栏自己出滚动条"，这样三栏能各滚各的，互不干扰。

### 31.5.5 颜色、字体与暗色主题

开发者工具类的界面通常是暗色的，我们的轨迹查看器也会是。用 **CSS 变量**统一管理颜色，方便以后换主题：

```css
:root {                  /* :root 代表整个文档，在这里定义的变量全局可用 */
  --bg: #0d1117;         /* 变量名以 -- 开头，这是背景色 */
  --fg: #e6edf3;         /* 前景（文字）色 */
  --danger: #f85149;     /* 表示"失败/危险"的红色 */
}
body {
  background: var(--bg); /* 用 var() 引用上面定义的变量 */
  color: var(--fg);
  font-family: -apple-system, "PingFang SC", sans-serif;  /* 字体，从左往右挑第一个有的 */
  font-size: 14px;
  line-height: 1.6;      /* 行高 = 字号的 1.6 倍，读起来不挤 */
}
.failed { color: var(--danger); }   /* 失败的行用红色 */
```

CSS 变量是主题切换的基础：想做亮色主题，只要换一组 `:root` 里的变量值，全站颜色就跟着变。

### 31.5.6 一点点动画（锦上添花）

加一个鼠标悬停变色的小效果，成本一行，但"专业感"立刻上来：

```css
.event-row {
  transition: background-color 0.15s ease;   /* 背景色变化时，用 0.15 秒平滑过渡 */
}
.event-row:hover {
  background-color: #161b22;                 /* 鼠标悬停时换个背景色 */
}
```

## 31.6 综合实战：纯 HTML/CSS 的轨迹页面

把本章学的全部拼起来，做一个静态的轨迹查看页面（暂时不能交互，第 32 章给它"通电"）。先写 HTML 骨架：

```html
<!DOCTYPE html>
<html lang="zh-CN">
<head>
  <meta charset="UTF-8">
  <title>Run #42 · 轨迹查看器</title>
  <link rel="stylesheet" href="trace.css">
</head>
<body>
  <div class="app">
    <main class="timeline">
      <header class="run-header">
        <h1>修复 divide 函数的除零崩溃</h1>
        <span class="badge badge-success">成功</span>
        <span class="meta">claude-sonnet · 7 轮 · 48 秒</span>
      </header>

      <section class="turn">
        <h2 class="turn-title">第 1 轮</h2>
        <div class="event-row">
          <span class="tool-name">grep</span>
          <span class="summary">divide(  →  src/math.rs:12</span>
          <span class="duration">8ms</span>
        </div>
        <div class="event-row failed">
          <span class="tool-name">bash</span>
          <span class="summary">cargo test  →  1 个失败</span>
          <span class="duration">2.1s</span>
        </div>
      </section>
    </main>

    <aside class="inspector">
      <h2>事件详情</h2>
      <pre><code>$ cargo test
test math::div_by_zero ... FAILED</code></pre>
    </aside>
  </div>
</body>
</html>
```

再写 `trace.css`（这是骨架，剩下的留给你练习补全）：

```css
* { box-sizing: border-box; margin: 0; }
:root {
  --bg:#0d1117; --panel:#161b22; --fg:#e6edf3; --muted:#8b949e;
  --green:#3fb950; --red:#f85149; --border:#30363d;
}
body {
  background: var(--bg); color: var(--fg);
  font: 14px/1.6 -apple-system, "PingFang SC", sans-serif;
}
.app { display: flex; height: 100vh; }
.timeline { flex: 1; overflow-y: auto; padding: 24px; }
.inspector {
  width: 420px; flex: none; overflow-y: auto;
  border-left: 1px solid var(--border); padding: 24px; background: var(--panel);
}
.event-row {
  display: flex; align-items: center; gap: 10px;
  padding: 8px 12px; border: 1px solid var(--border);
  border-radius: 6px; margin: 6px 0;
  transition: background .15s;
}
.event-row:hover { background: var(--panel); }
.event-row.failed { border-color: var(--red); }   /* 同时有 event-row 和 failed 两个 class 的行 */
.summary { flex: 1; color: var(--muted); }
```

保存、用 Live Server 打开，你就有了一个像模像样的暗色轨迹页面。

## 31.7 开发者工具：花十分钟练熟它

DevTools 是前端工程师的"手术台"。做完这套小练习，你的熟练度就超过一半初学者了：

1. **Elements**：打开任意网站，找到顶部某段文字的标签，双击改掉它的文字（只改你本地显示，刷新就还原）；
2. **盒模型**：选中任意按钮，看右下角它的盒模型四层数值；
3. **Network**：刷新页面，看左边列出的一条条请求——这就是 31.1 说的"请求"真实发生；
4. **手机预览**：点 DevTools 里的手机图标，看页面在手机尺寸下的样子；
5. **Console**：输入 `document.querySelector("h1")` 回车——这是下一章的预告：用代码抓住一个 DOM 元素。

## 31.8 小结与练习

- 浏览器把服务器发来的文本"画"成页面；前端三巨头分工：HTML 管结构、CSS 管外观、JS 管行为。
- HTML 全是嵌套的标签，嵌套关系构成 DOM 树；`id` 唯一、`class` 可复用。
- CSS 选中元素再化妆；必背 `box-sizing: border-box`；布局主力是 Flexbox（父盒 `display:flex` + 孩子 `flex:1`）；颜色用 CSS 变量统一管。
- DevTools 是你的手术台：Elements 调结构样式、Network 看请求。

**练习**

1. 完成 31.6 的轨迹页面：至少 3 个"轮次"、10 个事件行，包含成功和失败两种状态，加上鼠标悬停反馈，左右两栏能各自独立滚动。
2. 给页面顶部加一个 4 个卡片的统计区（总耗时 / 轮数 / 工具数 / 状态）。提示：外层用 `display: flex` 或试试 `display: grid` + `grid-template-columns: repeat(4, 1fr)`（四等分）。
3. 做一个亮色主题：再定义一组颜色变量，给 `<body>` 手动加上 `class="light"` 时生效（怎么用 class 切换主题，下一章学会用 JS 自动切）。
4. 打开任意网站，用 Elements 面板把它的某个标题颜色改成红色——证明"任何网页都能被你解剖和修改"。

> **下一章**：给这个静态页面"通电"——用 JavaScript 让它能搜索、能过滤、能点击展开。前端真正好玩的部分开始了。
