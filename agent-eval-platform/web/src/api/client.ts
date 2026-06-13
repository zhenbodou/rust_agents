import { z } from "zod";
import {
  BatchSchema,
  RunSchema,
  DashboardSchema,
  TrendSchema,
  CompareReportSchema,
  parseEvent,
  type MaybeEvent,
} from "./schemas";

async function get<T>(url: string, schema: z.ZodType<T>): Promise<T> {
  const res = await fetch(url);
  if (!res.ok) throw new Error(`HTTP ${res.status}: ${await res.text()}`);
  return schema.parse(await res.json());
}

export const fetchBatches = () =>
  get(`/api/batches`, z.object({ items: z.array(BatchSchema) })).then((r) => r.items);

export const fetchRuns = (batchId: string) =>
  get(`/api/runs?batch=${batchId}`, z.object({ items: z.array(RunSchema) })).then((r) => r.items);

export const fetchRun = (runId: string) => get(`/api/runs/${runId}`, RunSchema);

export async function fetchTracePage(runId: string, offset: number) {
  const res = await fetch(`/api/runs/${runId}/trace?offset=${offset}&limit=500`);
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  const page = z
    .object({ events: z.array(z.unknown()), total: z.number(), next_offset: z.number().nullable() })
    .parse(await res.json());
  return { events: page.events.map(parseEvent), nextOffset: page.next_offset };
}

export const fetchDashboard = () =>
  get(`/api/stats/dashboard`, DashboardSchema);

export const fetchTrend = (days = 30, groupBy = "day") =>
  get(
    `/api/stats/trend?days=${days}&group_by=${groupBy}`,
    z.object({ points: z.array(TrendSchema) }),
  ).then((r) => r.points);

export const fetchCompare = (a: string, b: string) =>
  get(`/api/reports/compare?a=${a}&b=${b}`, CompareReportSchema);

/** SSE 订阅（ch37）：onEvent 逐条回调；返回取消函数 */
export function subscribeRun(
  runId: string,
  onEvent: (e: MaybeEvent) => void,
  onLagged: () => void,
): () => void {
  const es = new EventSource(`/api/runs/${runId}/stream`);
  es.onmessage = (msg) => onEvent(parseEvent(JSON.parse(msg.data)));
  es.addEventListener("lagged", onLagged); // 掉帧 → 调用方走 REST 全量补偿
  es.onerror = () => {
    /* EventSource 自动重连并带 Last-Event-ID */
  };
  return () => es.close();
}
