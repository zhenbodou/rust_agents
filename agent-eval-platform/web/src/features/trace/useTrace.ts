// 实时与历史统一视图（书 51.2）：历史分页拉全 + SSE 续接 + 按 seq 合并去重
import { useEffect, useMemo, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { fetchRun, fetchTracePage, subscribeRun } from "../../api/client";
import type { MaybeEvent } from "../../api/schemas";

export function useTrace(runId: string) {
  const { data: run, refetch } = useQuery({
    queryKey: ["runs", runId],
    queryFn: () => fetchRun(runId),
    refetchInterval: (q) =>
      ["queued", "leased", "running"].includes(q.state.data?.status ?? "") ? 2000 : false,
  });
  const isLive = ["queued", "leased", "running"].includes(run?.status ?? "");

  const [history, setHistory] = useState<MaybeEvent[]>([]);
  const [live, setLive] = useState<MaybeEvent[]>([]);
  const bufRef = useRef<MaybeEvent[]>([]);

  // 历史段：分页拉全
  useEffect(() => {
    let cancelled = false;
    (async () => {
      const all: MaybeEvent[] = [];
      let offset: number | null = 0;
      while (offset !== null) {
        const page = await fetchTracePage(runId, offset);
        all.push(...page.events);
        offset = page.nextOffset;
      }
      if (!cancelled) setHistory(all);
    })().catch(console.error);
    return () => { cancelled = true; };
  }, [runId, isLive]); // run 结束时再拉一次，保证终态完整

  // 实时段：SSE + 100ms 节流合帧（ch37）
  useEffect(() => {
    if (!isLive) return;
    const flush = setInterval(() => {
      if (bufRef.current.length) {
        const b = bufRef.current;
        bufRef.current = [];
        setLive((prev) => [...prev, ...b]);
      }
    }, 100);
    const unsubscribe = subscribeRun(
      runId,
      (e) => {
        bufRef.current.push(e);
        if (e.type === "run_finished") void refetch();
      },
      () => void refetch(), // lagged：触发 run 重查 → isLive 翻转 → 历史段全量重拉补偿
    );
    return () => { unsubscribe(); clearInterval(flush); };
  }, [runId, isLive, refetch]);

  // 合并：seq 单调，去重后排序
  const events = useMemo(() => {
    const seen = new Set<number>();
    const merged: MaybeEvent[] = [];
    for (const e of [...history, ...live]) {
      if (e.seq >= 0 && seen.has(e.seq)) continue;
      seen.add(e.seq);
      merged.push(e);
    }
    return merged.sort((a, b) => a.seq - b.seq);
  }, [history, live]);

  return { run, events, isLive };
}
