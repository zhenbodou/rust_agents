# 第 37 章 流式 UI：让 Agent 的输出实时蹦出来

> Agent 跑一次要几十秒甚至几分钟。如果用户盯着一个转圈图标干等，体验很糟。好的 Agent 前端会**实时**把"它正在干什么"一点点显示出来——就像 ChatGPT 回答时文字一个个蹦出来。本章打通"Rust 后端 → 浏览器"的实时数据通道，这是轨迹查看器和所有 Agent 前端的地基。我们从"为什么需要实时、有哪几种实现"讲起。

## 37.1 三种"实时"方案，先学会选

普通网页是"你问一次、服务器答一次"。但 Agent 场景需要服务器**主动、持续**地往浏览器推消息。实现这个有三种方案，先看怎么选：

| 方案 | 方向 | 类比 | 适合 |
|---|---|---|---|
| **轮询** | 浏览器反复问 | 每隔几秒问一句"好了吗？" | 低频状态（run 列表） |
| **SSE** | 服务器单向推 | 电台直播：你只管收听 | 轨迹流、日志流、token 流 |
| **WebSocket** | 双向 | 打电话：两边都能说话 | 用户要中途打断/输入的交互式会话 |

**经验法则**：只是"看 Agent 在干嘛"（单向）用 **SSE**；用户要能中途插话、打断 Agent（双向）用 **WebSocket**；普通列表页用上一章的 TanStack Query 轮询。ChatGPT、Claude 的聊天流，用的都是 SSE。本章重点讲 SSE，最后简单带 WebSocket。

## 37.2 SSE 是什么：一个"不挂断"的响应

SSE（Server-Sent Events，服务器发送事件）的原理简单得出人意料：**就是一个一直不关闭的 HTTP 响应**。普通响应是"发完内容就结束"，SSE 是"连接保持着，服务器有新消息就往这条连接里写一行"。

它的数据格式是纯文本，按这种格式分条：

```
event: tool_call
data: {"type":"tool_call","toolName":"bash"}

event: llm_chunk
data: {"type":"llm_chunk","delta":"正在分析"}

id: 42
```

规则就几条：`event:` 是这条消息的类型，`data:` 是内容（通常放 JSON），`id:` 是序号；**每条消息之间用一个空行隔开**。那个 `id` 有个重要作用——网络断了重连时，浏览器会自动带上"我收到的最后一个 id"，服务器就能从那之后接着发，不丢数据。**有没有处理这个 id，是"生产级"和"玩具"的分界线。**

## 37.3 后端：用 Rust（axum）推 SSE

回到你熟悉的 Rust。思路是：每个正在运行的 run 配一个"广播频道"，Agent 主循环每产生一个事件就往频道里 `send`，任意多个浏览器都能订阅这个频道收消息。

```rust
use axum::response::sse::{Event, KeepAlive, Sse};
use tokio_stream::{wrappers::BroadcastStream, StreamExt};

// 浏览器请求 /api/runs/:id/stream 时调用这个函数
pub async fn stream_run(
    State(hub): State<Arc<RunHub>>,
    Path(run_id): Path<RunId>,
) -> Sse<impl Stream<Item = Result<Event, axum::Error>>> {
    let rx = hub.subscribe(&run_id);                 // 订阅这个 run 的广播频道
    let stream = BroadcastStream::new(rx).filter_map(|msg| {
        let event = msg.ok()?;
        let data = serde_json::to_string(&event).ok()?;
        Some(Ok(Event::default()
            .event(event.kind_str())                 // 对应 event: tool_call
            .id(event.seq.to_string())               // 对应 id: 42（断线续传锚点）
            .data(data)))
    });
    // keep_alive 是"心跳"：每 15 秒发个空信号，防止中间的代理以为连接空闲把它掐断
    Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
}
```

三个生产经验，踩过坑的人才知道：

- **心跳（KeepAlive）必须有**：否则 nginx、K8s 网关这些"中间人"会觉得连接闲置，几十秒后就掐断它。
- **网关要关掉缓冲**：nginx/ingress 默认会"攒一批再发"，这会让 SSE 变成"憋到最后一次性全吐出来"，实时性全没了。要配 `proxy_buffering off`——这是经典生产事故。
- **广播频道容量要够**（比如 1024）：消费慢的浏览器可能跟不上、丢消息，这时通知它去调一个普通接口把完整数据补拉回来。

## 37.4 前端：接收 SSE

浏览器原生有个 `EventSource` 能收 SSE，但它有个硬伤——不能带自定义请求头（也就带不了登录令牌）。所以生产里常用一个封装好的小库 `@microsoft/fetch-event-source`：

```typescript
import { fetchEventSource } from "@microsoft/fetch-event-source";

function subscribeRun(runId: string, onEvent: (e: TraceEvent) => void, signal: AbortSignal) {
  fetchEventSource(`/api/runs/${runId}/stream`, {
    signal,
    headers: { Authorization: `Bearer ${getToken()}` },   // 能带登录令牌
    onmessage(msg) {
      onEvent(JSON.parse(msg.data));        // 每收到一条就回调
    },
    onerror(err) {
      console.warn("连接出错，库会自动重连", err);   // 它会自动指数退避重连
    },
    openWhenHidden: true,    // 切到别的标签页也不断流（评测可能跑很久）
  });
}
```

照例把它封装成一个 React Hook，方便组件用（结合上一章的"节流"思想，攒一小批再更新，避免高频重画）：

```tsx
function useRunStream(runId: string) {
  const [events, setEvents] = useState<TraceEvent[]>([]);
  const [status, setStatus] = useState<"connecting" | "live" | "done">("connecting");

  useEffect(() => {
    const ctrl = new AbortController();
    let buf: TraceEvent[] = [];
    // 每 80 毫秒把攒下的一批一次性更新进去，而不是每条都触发重画
    const timer = setInterval(() => {
      if (buf.length) { setEvents((p) => [...p, ...buf]); buf = []; }
    }, 80);

    subscribeRun(runId, (e) => {
      buf.push(e);
      setStatus(e.type === "run_finished" ? "done" : "live");
    }, ctrl.signal);

    return () => { ctrl.abort(); clearInterval(timer); };   // 组件卸载时断开 + 清理
  }, [runId]);

  return { events, status };
}
```

## 37.5 渲染打字机效果：两个真实的坑

让文字像打字机一样出现，看似简单，但有两个坑：

**坑一：流到一半的 Markdown 是"残缺"的**。模型输出代码时是一个字一个字来的，某一刻你手里可能是 ```` ```rust ```` 但还没等到结尾的 ```` ``` ````。直接渲染会让后面所有内容都被当成代码块。处理办法是"临时补全"：

```tsx
function StreamingMessage({ text }: { text: string }) {
  // 数一下有几个 ``` ，如果是奇数，说明有个代码块没闭合，临时补一个
  const fenceCount = (text.match(/```/g) ?? []).length;
  const safe = fenceCount % 2 === 1 ? text + "\n```" : text;
  return <Markdown>{safe}</Markdown>;
}
```

**坑二：自动滚动要"懂规矩"**。新内容来了应该自动滚到底，但如果用户正在往上翻看历史，你就不该硬把他拽回底部。规则是"用户在底部附近时才自动滚"：

```tsx
function useAutoScroll(dep: unknown) {
  const ref = useRef<HTMLDivElement>(null);
  const pinned = useRef(true);                  // 当前是否"贴在底部"
  useEffect(() => {
    const el = ref.current;
    if (el && pinned.current) el.scrollTop = el.scrollHeight;   // 贴底时才自动滚
  }, [dep]);
  const onScroll = () => {
    const el = ref.current!;
    // 离底部 40 像素内算"贴底"，用户往上翻就解除
    pinned.current = el.scrollHeight - el.scrollTop - el.clientHeight < 40;
  };
  return { ref, onScroll };
}
```

## 37.6 WebSocket：当用户需要"插话"

如果用户要**中途打断 Agent、回答 Agent 的提问**（比如 Agent 问"要执行这个危险命令吗？"），单向的 SSE 就不够了，得用双向的 WebSocket。

设计协议时，两个方向的消息都用上一章的"判别联合"标好类型：

```typescript
// 浏览器 → 服务器
type ClientMsg =
  | { type: "user_input"; text: string }
  | { type: "interrupt" }                                          // 打断
  | { type: "permission_response"; callId: string; approved: boolean };  // 批准/拒绝

// 服务器 → 浏览器
type ServerMsg =
  | TraceEvent
  | { type: "permission_request"; callId: string; tool: string };   // 请求授权
```

Rust 端用 `tokio::select!` 同时处理两个方向（这正是你 Part 5 学的并发）：

```rust
loop {
    tokio::select! {
        // 方向一：Agent 有新消息 → 发给浏览器
        Some(msg) = rx.recv() => {
            sink.send(Message::Text(serde_json::to_string(&msg)?)).await?;
        }
        // 方向二：浏览器发来消息 → 转给 Agent
        Some(Ok(msg)) = stream.next() => {
            match serde_json::from_str::<ClientMsg>(msg.to_text()?)? {
                ClientMsg::Interrupt => agent.cancel(),
                ClientMsg::UserInput { text } => agent.send_input(text).await,
                _ => {}
            }
        }
        else => break,
    }
}
agent.cancel();   // 浏览器一断开就停掉 Agent，别让它在后台空跑烧钱
```

最后那句 `agent.cancel()` 很关键：用户关掉页面后要立刻停 Agent，否则它在后台继续调 LLM 白白花钱。

## 37.7 防卡顿：三层节流

LLM 输出可能每秒上百个片段，如果每个都触发一次界面更新，页面会卡。三层防护：

1. **后端合并**：把 50 毫秒内的多个 token 片段合成一条再发；
2. **前端节流**：80–100 毫秒攒一批再更新界面（就是 37.4 那个 `buf` 模式）；
3. **渲染降级**：流式过程中只显示纯文本，等 Agent 完全结束后再整体渲染一次带高亮的 Markdown。

## 37.8 小结与练习

- 单向看流用 SSE（记得心跳 + 关网关缓冲 + 用 id 做断线续传），双向交互用 WebSocket。
- SSE 本质是"不关闭的 HTTP 响应"，按 `event:`/`data:`/`id:` 分条。
- 打字机渲染两个坑：临时补全未闭合的 Markdown、自动滚动要让位给手动上翻。
- 防卡顿三层：后端合帧、前端节流、流式期间降级渲染。

**练习**

1. 给 mini-claude-code 加一个 `--serve` 模式：用 axum 暴露一个 WebSocket 接口，做一个最简单的网页聊天界面，能发消息、能看到回复一个字一个字蹦出来。
2. 实现"丢帧补偿"：故意把后端广播容量调到很小（比如 8），验证前端能发现自己跟不上、然后去拉一次全量数据补齐。
3. 写一个脚本以每秒 200 条的速度推流，用 React DevTools Profiler 证明你的节流把界面更新频率压在了每秒十几次的合理范围。

> **下一章**：把本章的流式能力和前面所有前端知识汇总起来，做出 Agent Infra 团队最常被要求的那个工具——专业级轨迹查看器。这是你简历上的加分项本身。
