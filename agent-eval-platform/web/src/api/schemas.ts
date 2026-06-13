// 与 schemas/trace-event.schema.json 对齐（schema_version=1）。
// 生产做法：json-schema-to-zod 生成；教学版手写 + 未知事件优雅降级（ch51）。
import { z } from "zod";

const base = { schema_version: z.number(), seq: z.number(), ts: z.number() };

export const TraceEventSchema = z.discriminatedUnion("type", [
  z.object({ ...base, type: z.literal("run_started"), run_id: z.string(),
             case_id: z.string(), task: z.string(), model: z.string().nullish() }),
  z.object({ ...base, type: z.literal("llm_request"), turn: z.number(),
             input_tokens: z.number().default(0) }),
  z.object({ ...base, type: z.literal("llm_chunk"), turn: z.number(), delta: z.string() }),
  z.object({ ...base, type: z.literal("tool_call"), turn: z.number(),
             call_id: z.string(), tool_name: z.string(), args: z.unknown() }),
  z.object({ ...base, type: z.literal("tool_result"), call_id: z.string(),
             output: z.string(), is_error: z.boolean().default(false),
             duration_ms: z.number().default(0) }),
  z.object({ ...base, type: z.literal("run_finished"),
             status: z.enum(["passed", "failed", "error", "timeout"]),
             score: z.number().nullish(), cost_usd: z.number().nullish(),
             turns: z.number().default(0) }),
]);

export type TraceEvent = z.infer<typeof TraceEventSchema>;

/** 未知事件类型 → 降级占位而不是崩（前端必须假设后端可能更新）*/
export type MaybeEvent = TraceEvent | { type: "unknown"; seq: number; raw: unknown };

export function parseEvent(raw: unknown): MaybeEvent {
  const r = TraceEventSchema.safeParse(raw);
  if (r.success) return r.data;
  const seq = typeof (raw as { seq?: number })?.seq === "number" ? (raw as { seq: number }).seq : -1;
  return { type: "unknown", seq, raw };
}

export const RunSchema = z.object({
  id: z.string(),
  batch_id: z.string(),
  case_id: z.string(),
  status: z.string(),
  score: z.number().nullish(),
  cost_usd: z.number().nullish(),
  turns: z.number().nullish(),
});
export type Run = z.infer<typeof RunSchema>;

export const BatchSchema = z.object({
  id: z.string(),
  name: z.string(),
  status: z.string(),
  passed: z.number(),
  total: z.number(),
  cost_usd: z.number().nullish(),
  pass_rate: z.number().nullish(),
  created_at: z.string(),
});
export type Batch = z.infer<typeof BatchSchema>;

export const DashboardSchema = z.object({
  total_runs: z.number(),
  total_batches: z.number(),
  total_cost_usd: z.number(),
  pass_rate: z.number(),
  runs_last_24h: z.number(),
  queue_depth: z.number(),
  active_runs: z.number(),
  status_breakdown: z.record(z.number()),
});
export type Dashboard = z.infer<typeof DashboardSchema>;

export const TrendSchema = z.object({
  time: z.string(),
  passed: z.number(),
  total: z.number(),
  cost_usd: z.number(),
  pass_rate: z.number(),
});
export type TrendPoint = z.infer<typeof TrendSchema>;

const CaseVerdictSchema = z.object({
  case_id: z.string(),
  status_a: z.string(),
  status_b: z.string(),
  score_a: z.number().nullish(),
  score_b: z.number().nullish(),
  run_a: z.string(),
  run_b: z.string(),
  verdict: z.enum(["regression", "improvement", "same"]),
});

export const CompareReportSchema = z.object({
  comparability_warnings: z.array(z.string()),
  summary: z.object({
    total: z.number(),
    passed_a: z.number(),
    passed_b: z.number(),
    pass_rate_a: z.number(),
    pass_rate_b: z.number(),
    regressions: z.number(),
    improvements: z.number(),
  }),
  cases: z.array(CaseVerdictSchema),
});
export type CompareReport = z.infer<typeof CompareReportSchema>;
export type CaseVerdict = z.infer<typeof CaseVerdictSchema>;
