# 第 39 章 后端基础：HTTP、API 与数据库从零开始

> 前面 Part 7 你做的前端，一直在 `fetch("/api/runs")` 找一个"后端"要数据。这个后端到底是什么？本章把这层窗户纸捅破：什么是服务器、HTTP 是什么、API 怎么设计、数据存哪。我假设你**完全没接触过后端**，从最基础的类比讲起。这些是 Part 10 评测平台的地基，也是任何全栈岗位的常识底线。

## 39.1 后端是什么：餐厅的后厨

用餐厅类比最清楚。你在 Part 7 做的前端，是**前厅**——顾客（用户）能看到、能点菜的地方。但真正做菜、管食材的是**后厨**——这就是后端。

```
前端（顾客的桌子，在用户的浏览器里）          后端（后厨，在远处的服务器上）
  点菜单：fetch("/api/runs")   ──请求──►     收到订单 → 处理 → 查冰箱（数据库）
  上菜：把返回的数据显示出来   ◄──响应──     把做好的菜端出来
```

**为什么必须有后端？** 三个原因，缺一不可：

1. **数据要长久保存**：前端的数据在用户浏览器里，关掉就没了。要让数据"明天还在"，必须存在后端的数据库里。
2. **逻辑要可信**：前端代码用户能随便改（按 F12 就能看源码）。涉及钱、权限的计算，必须放在用户碰不到的后端。
3. **密钥要保密**：你的 Anthropic API key 如果放前端，等于公开送人。它只能藏在后端。

"服务器"这个词有三层意思，别被绕晕：一台物理机器、机器上一个一直在运行的程序、或"提供服务的一方"。日常说"起个服务器"，指的是第二种——一个一直跑着、等着接收请求的程序。你现在就能起一个最简单的：

```bash
cd my-first-page && python3 -m http.server 8000
# 打开浏览器访问 http://localhost:8000，你的电脑此刻既是顾客又是后厨
```

这里 `localhost` 表示"本机"，`8000` 是**端口号**。一台机器上可以同时跑很多服务，靠端口号区分——就像一栋楼有很多房间，IP 地址是楼的门牌，端口是房间号。

## 39.2 HTTP：前后端之间的"点单小票"

前端和后端之间怎么传话？靠 HTTP 协议。别被"协议"吓到——HTTP 其实就是**有固定格式的文本**，像餐厅的点单小票。我们看一次真实的请求和响应长什么样：

```
─── 前端发出的"请求" ───────────────────
GET /api/runs?status=failed HTTP/1.1     ← 想干嘛(GET=查) + 找什么(路径) 
Host: eval.example.com                   ← 一些附加信息（叫"头部"）
Authorization: Bearer sk-xxx             ← 我的身份凭证
                                         ← 空行，表示头部结束
─── 后端返回的"响应" ───────────────────
HTTP/1.1 200 OK                          ← 状态码 200 = 成功
Content-Type: application/json           ← 我返回的是 JSON 格式
                                         
{"runs":[{"id":"run-42","status":"failed"}]}   ← 真正的数据（叫"响应体"）
```

就这么简单。你可以亲手发一个请求来体会——命令行工具 `curl` 是后端工程师的听诊器：

```bash
curl -v https://api.github.com/repos/rust-lang/rust    # -v 显示完整的请求和响应
```

### 39.2.1 方法：你想对资源做什么

请求开头那个词（GET）叫"方法"，表示你想干嘛。常用五个：

| 方法 | 意思 | 重复发会怎样 |
|---|---|---|
| GET | 读取（查看） | 没事，读多少次都一样 |
| POST | 创建（新增） | **危险**：发两次会创建两个！ |
| PUT | 整体替换 | 没事，结果一样 |
| PATCH | 局部修改 | |
| DELETE | 删除 | 没事，删了就是删了 |

注意 POST 那行"发两次会创建两个"。这引出一个重要概念**幂等性**——"发 N 次和发 1 次效果一样"的方法是幂等的。为什么重要？网络超时后客户端要不要重发？幂等的可以放心重发，POST 重发可能重复下单。第 50 章评测平台用"幂等键"解决的就是这个问题。

### 39.2.2 状态码：后端的"回执"

响应里那个数字（200）是状态码，告诉你结果如何。按首位数字分五类，记住这些就够用：

```
2xx 成功    200 成功 | 201 已创建（POST 成功）
4xx 你错了  400 请求格式不对 | 401 没登录 | 403 登录了但没权限 | 404 找不到 | 429 请求太频繁
5xx 我错了  500 服务器内部出错 | 502/503 后端挂了/过载
```

排查问题的路标：看到 **4xx 先查自己的请求哪里不对，5xx 是服务方的锅**。

### 39.2.3 CORS：前端开发一定会撞的墙

浏览器有个安全规定：网页里的 JS 默认只能请求**和自己同源**（同域名同端口）的地址。所以你本地 `localhost:5173` 的前端去请求 `localhost:8080` 的后端，会被浏览器拦下来报"CORS 错误"。

两个解法：后端在响应里加一个 `Access-Control-Allow-Origin` 头说"我允许它"；或者开发时用 Vite 代理转发（你在第 36 章配的 `server.proxy` 就是干这个的——现在你懂它解决什么了）。记住一点：**CORS 是浏览器的限制，用 curl 不受影响**——所以前端报 CORS 错时，去查后端响应头和代理配置。

## 39.3 REST：一套设计 API 的好习惯

API 就是后端对外提供的"功能列表"。REST 是设计 API 的一套约定，核心思想一句话：**用 URL 表示"东西"（名词），用方法表示"操作"（动词）**。

```
GET    /api/runs           列出所有运行
POST   /api/runs           创建一个运行
GET    /api/runs/run-42    查看某一个
PATCH  /api/runs/run-42    修改某一个
DELETE /api/runs/run-42    删除某一个
GET    /api/runs/run-42/events   某个运行的事件（子资源）
```

对比一下糟糕的设计：`GET /api/getRunList`、`POST /api/deleteRun?id=42`（把动词塞进 URL）。这也能用，但每个接口都得重新学一遍，很乱。REST 风格统一，谁来都能猜到怎么用。

返回的数据也有约定，比如列表带上分页信息、错误用统一格式：

```json
// 列表
{ "items": [ {...}, {...} ], "total": 137, "page": 2 }

// 出错时：给一个机器能识别的 code + 给人看的 message
{ "error": { "code": "RUN_NOT_FOUND", "message": "run 42 不存在" } }
```

**API 是一份"契约"**——前端和后端约定好接口长什么样，各自独立开发。一旦改了契约可能弄坏所有调用方，所以有"版本化、只加不删字段"等纪律（第 49 章会深入）。

## 39.4 数据库：带索引的智能文件柜

### 39.4.1 为什么不直接用文件存

你可能想：数据存个文件不就行了？mini-claude-code 的 session 确实存的是文件，因为它单用户、单进程。但多用户的后端用文件会出三个大问题：多个人同时写同一个文件会互相覆盖；想"找出所有失败且贵的运行"得把整个文件翻一遍；写到一半程序崩了文件就坏了。

数据库就是为解决这三件事而生的智能文件柜：**能处理并发、能快速查找（靠索引）、崩溃也不会损坏（靠事务）**。

关系型数据库（如 PostgreSQL、SQLite）把数据存成**表格**——像 Excel，有行有列，每列规定了类型。我们用最简单的 SQLite 上手（零安装、就一个文件）：

```bash
sqlite3 eval.db    # macOS/Linux 自带，直接进入
```

### 39.4.2 SQL：和数据库对话的语言

操作数据库用 SQL 语言。它读起来很像英语，我们从增删改查学起：

```sql
-- 建一张表：规定有哪些列、什么类型
CREATE TABLE runs (
    id         TEXT PRIMARY KEY,        -- 主键：每行的唯一身份证
    case_id    TEXT NOT NULL,           -- NOT NULL：这列不能空
    status     TEXT NOT NULL DEFAULT 'queued',
    cost_usd   REAL                     -- REAL = 小数；没填就是空(NULL)
);

-- 增：插入数据
INSERT INTO runs (id, case_id, status, cost_usd) VALUES
  ('run-2', 'fix-div',  'passed', 0.031),
  ('run-3', 'add-test', 'failed', 0.087);

-- 查：SELECT 是 SQL 的核心，一句一句读就懂
SELECT id, status, cost_usd     -- 要哪几列
FROM runs                       -- 从哪张表
WHERE status = 'failed'         -- 筛选条件
ORDER BY cost_usd DESC          -- 按成本从高到低排
LIMIT 20;                       -- 只要前 20 条

-- 改 / 删（千万记得写 WHERE！不写会作用于整张表）
UPDATE runs SET status = 'passed' WHERE id = 'run-3';
DELETE FROM runs WHERE id = 'run-3';

-- 统计：把"算账"交给数据库做，比取回来自己算快得多
SELECT status, COUNT(*) AS 数量, AVG(cost_usd) AS 平均成本
FROM runs
GROUP BY status;                -- 按状态分组分别统计
```

> **血泪警告**：`UPDATE` 和 `DELETE` 不写 `WHERE` 会作用于**整张表**。改删之前，先用同样的 `WHERE` 跑个 `SELECT` 确认选中的是不是你想动的那些行。

### 39.4.3 多表关联（JOIN）

真实数据会拆成多张表，用 id 互相关联（避免重复存同一信息——和 React"派生数据不另存"是同一个道理）。比如批次一张表、运行一张表，运行属于某个批次：

```sql
-- JOIN：把两张表按关联条件拼起来查 ——"每个模型的平均分"
SELECT b.model, AVG(r.score) AS 平均分
FROM runs r
JOIN batches b ON r.batch_id = b.id    -- 按 batch_id 把两表的行配对
GROUP BY b.model;
```

### 39.4.4 索引和事务

两个让数据库"又快又稳"的关键：

```sql
-- 索引：给某列建一个"目录"，查这列时从"逐行翻"变成"直接跳"，快几个数量级
CREATE INDEX idx_runs_status ON runs(status);
-- 代价是写入稍慢、占点空间，所以按真实查询需求建，别每列都建

-- 事务：一组操作"要么全成功，要么全不做"（转账的经典例子）
BEGIN;
UPDATE accounts SET balance = balance - 100 WHERE id = 'a';
UPDATE accounts SET balance = balance + 100 WHERE id = 'b';
COMMIT;        -- 万一中间崩了，自动回滚，钱不会凭空消失
```

**选型一句话**：自己玩、嵌入式用 SQLite；正经服务端默认用 PostgreSQL（功能最全，评测平台用它）。

## 39.5 动手：写一个最小后端

把前端、后端、数据库首次连起来。我们用 Node.js（你已会 JS）写一个真的 API 服务器：

```javascript
// server.mjs —— 用 node server.mjs 运行（Node 22+，不需要装任何东西）
import { createServer } from "node:http";
import { DatabaseSync } from "node:sqlite";

const db = new DatabaseSync("eval.db");
db.exec(`CREATE TABLE IF NOT EXISTS runs (
  id TEXT PRIMARY KEY, case_id TEXT NOT NULL, status TEXT DEFAULT 'queued')`);

const server = createServer(async (req, res) => {
  const url = new URL(req.url, `http://${req.headers.host}`);
  res.setHeader("Content-Type", "application/json");
  res.setHeader("Access-Control-Allow-Origin", "*");     // 允许跨源（演示用）

  // GET /api/runs —— 查询列表
  if (req.method === "GET" && url.pathname === "/api/runs") {
    const rows = db.prepare("SELECT * FROM runs").all();
    res.end(JSON.stringify({ items: rows }));

  // POST /api/runs —— 创建一个
  } else if (req.method === "POST" && url.pathname === "/api/runs") {
    let body = "";
    for await (const chunk of req) body += chunk;        // 读取请求体
    const { caseId } = JSON.parse(body);
    if (!caseId) {                                       // 校验输入
      res.statusCode = 400;
      return res.end(JSON.stringify({ error: { code: "MISSING_FIELD", message: "缺 caseId" } }));
    }
    const id = `run-${crypto.randomUUID().slice(0, 8)}`;
    db.prepare("INSERT INTO runs (id, case_id) VALUES (?, ?)").run(id, caseId);
    res.statusCode = 201;
    res.end(JSON.stringify({ id }));

  } else {
    res.statusCode = 404;
    res.end(JSON.stringify({ error: { code: "NOT_FOUND", message: url.pathname } }));
  }
});

server.listen(8080, () => console.log("API 跑在 http://localhost:8080"));
```

```bash
node server.mjs
curl -X POST localhost:8080/api/runs -d '{"caseId":"fix-div"}'   # 创建，返回 {"id":"run-xxxx"}
curl localhost:8080/api/runs                                      # 查询，返回 {"items":[...]}
```

然后把第 35 章的 React 应用指向这个后端 fetch 数据——**你的第一个全栈应用就成了**。

### 后端安全第一课：SQL 注入

注意上面代码插入数据时用的是 `?` 占位符，把值单独传进去。**绝对不要**用字符串拼接 SQL：

```javascript
db.exec(`SELECT * FROM runs WHERE status = '${status}'`);   // 💥 千万别这么写
// 攻击者把 status 传成  x' OR '1'='1  ，条件就恒为真，整张表泄露；
// 传个 '; DROP TABLE runs; --  甚至能删你的表
```

**所有用户输入都要走 `?` 占位符（参数化查询）**。这和前端用 `textContent`（第 32.8 防 XSS）是同一个原则在不同层的体现：**数据就是数据，永远别让它变成被执行的代码**。

## 39.6 小结与练习

- 后端 = 持久化数据 + 可信逻辑 + 保管密钥；IP 定位机器，端口定位程序。
- HTTP 是文本协议：方法（GET/POST…及幂等性）、状态码（4xx 你错/5xx 我错）；CORS 是浏览器限制。
- REST：URL 是名词、方法是动词；API 是契约。
- 数据库解决并发、查询、崩溃安全；SQL 的增删改查 + JOIN + 索引 + 事务；用 `?` 占位符防 SQL 注入。

**练习**

1. 用 curl 对 39.5 的服务器创建 5 个 run、查询列表，并故意制造一个 400 和一个 404。
2. 给服务器加三个接口：`GET /api/runs/:id`（找不到返回 404）、`PATCH /api/runs/:id`（改 status）、`GET /api/stats`（按 status 分组统计）。
3. 在 sqlite3 里练习：查每个 case 的通过率、成本最高的 3 个 run、被跑过超过 1 次的 case。
4. 把第 35 章 React 应用的数据源换成这个后端，再加一个"创建 run"的表单（提交后刷新列表）——完成你的第一个全栈应用。

> **下一章**：Python 从零开始。算法团队的世界（RL 训练、评测脚本、各种 Agent 框架）全是 Python，它也是后端开发的另一大主力语言。
