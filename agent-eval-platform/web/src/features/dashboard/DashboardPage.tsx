/**
 * 仪表盘页面（书第 51 章：前端实时监控）
 *
 * 布局：
 *   顶部 4 个统计卡片（总 run / 通过率 / 成本 / 活跃队列）
 *   中部 通过率趋势折线图（recharts LineChart，数据来自 /api/stats/trend）
 *   底部 最近批次列表
 */

import { useQuery } from "@tanstack/react-query";
import { Link } from "react-router-dom";
import {
  CartesianGrid,
  Legend,
  Line,
  LineChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import { fetchBatches, fetchDashboard, fetchTrend } from "../../api/client";
import type { TrendPoint } from "../../api/schemas";

// ─── 统计卡片 ──────────────────────────────────────────────────────────────

function StatCard({
  label,
  value,
  sub,
  color,
}: {
  label: string;
  value: string | number;
  sub?: string;
  color?: string;
}) {
  return (
    <div className="stat-card" style={{ borderTop: `3px solid ${color ?? "#6366f1"}` }}>
      <div className="stat-label">{label}</div>
      <div className="stat-value">{value}</div>
      {sub && <div className="stat-sub">{sub}</div>}
    </div>
  );
}

// ─── 趋势图 ────────────────────────────────────────────────────────────────

function TrendChart({ points }: { points: TrendPoint[] }) {
  const data = points.map((p) => ({
    time: p.time.slice(0, 10), // "2025-01-01"
    pass_rate: +(p.pass_rate * 100).toFixed(1),
    total: p.total,
    cost: +p.cost_usd.toFixed(4),
  }));

  return (
    <div className="chart-card">
      <h3 className="chart-title">通过率趋势（过去 30 天）</h3>
      <ResponsiveContainer width="100%" height={260}>
        <LineChart data={data} margin={{ top: 8, right: 24, left: 0, bottom: 0 }}>
          <CartesianGrid strokeDasharray="3 3" stroke="#e5e7eb" />
          <XAxis dataKey="time" tick={{ fontSize: 11 }} />
          <YAxis
            yAxisId="rate"
            domain={[0, 100]}
            tickFormatter={(v) => `${v}%`}
            tick={{ fontSize: 11 }}
          />
          <YAxis
            yAxisId="count"
            orientation="right"
            tick={{ fontSize: 11 }}
          />
          <Tooltip
            formatter={(val: number, name: string) =>
              name === "通过率" ? `${val}%` : val
            }
          />
          <Legend />
          <Line
            yAxisId="rate"
            type="monotone"
            dataKey="pass_rate"
            name="通过率"
            stroke="#6366f1"
            dot={false}
            strokeWidth={2}
          />
          <Line
            yAxisId="count"
            type="monotone"
            dataKey="total"
            name="Run 数"
            stroke="#10b981"
            dot={false}
            strokeWidth={1.5}
            strokeDasharray="4 2"
          />
        </LineChart>
      </ResponsiveContainer>
    </div>
  );
}

// ─── 成本趋势图 ────────────────────────────────────────────────────────────

function CostChart({ points }: { points: TrendPoint[] }) {
  const data = points.map((p) => ({
    time: p.time.slice(0, 10),
    cost: +p.cost_usd.toFixed(4),
  }));

  return (
    <div className="chart-card">
      <h3 className="chart-title">API 成本趋势（USD）</h3>
      <ResponsiveContainer width="100%" height={200}>
        <LineChart data={data} margin={{ top: 8, right: 24, left: 0, bottom: 0 }}>
          <CartesianGrid strokeDasharray="3 3" stroke="#e5e7eb" />
          <XAxis dataKey="time" tick={{ fontSize: 11 }} />
          <YAxis tickFormatter={(v) => `$${v}`} tick={{ fontSize: 11 }} />
          <Tooltip formatter={(v: number) => `$${v}`} />
          <Line
            type="monotone"
            dataKey="cost"
            name="成本 (USD)"
            stroke="#f59e0b"
            dot={false}
            strokeWidth={2}
          />
        </LineChart>
      </ResponsiveContainer>
    </div>
  );
}

// ─── 批次列表 ────────────────────────────────────────────────────────────

const STATUS_COLOR: Record<string, string> = {
  running: "#6366f1",
  done: "#10b981",
  cancelled: "#9ca3af",
  pending: "#f59e0b",
};

function RecentBatches() {
  const { data, isLoading } = useQuery({
    queryKey: ["batches"],
    queryFn: fetchBatches,
    refetchInterval: 10_000,
  });

  if (isLoading) return <div className="loading">加载中…</div>;
  const batches = (data ?? []).slice(0, 10);

  return (
    <div className="card">
      <h3 className="card-title">最近批次</h3>
      <table className="table">
        <thead>
          <tr>
            <th>名称</th>
            <th>状态</th>
            <th>通过率</th>
            <th>成本</th>
            <th>时间</th>
          </tr>
        </thead>
        <tbody>
          {batches.map((b) => (
            <tr key={b.id}>
              <td>
                <Link to={`/batches/${b.id}`}>{b.name}</Link>
              </td>
              <td>
                <span
                  className="badge"
                  style={{ background: STATUS_COLOR[b.status] ?? "#9ca3af" }}
                >
                  {b.status}
                </span>
              </td>
              <td>
                {b.total > 0
                  ? `${b.passed}/${b.total} (${((b.passed / b.total) * 100).toFixed(0)}%)`
                  : "—"}
              </td>
              <td>{b.cost_usd != null ? `$${b.cost_usd.toFixed(4)}` : "—"}</td>
              <td className="muted">{b.created_at.slice(0, 16).replace("T", " ")}</td>
            </tr>
          ))}
          {batches.length === 0 && (
            <tr>
              <td colSpan={5} className="empty">还没有批次，先创建一个吧</td>
            </tr>
          )}
        </tbody>
      </table>
    </div>
  );
}

// ─── 页面主体 ────────────────────────────────────────────────────────────

export function DashboardPage() {
  const { data: dash, isLoading: dashLoading } = useQuery({
    queryKey: ["dashboard"],
    queryFn: fetchDashboard,
    refetchInterval: 15_000,
  });

  const { data: trend = [] } = useQuery({
    queryKey: ["trend", 30],
    queryFn: () => fetchTrend(30, "day"),
    refetchInterval: 60_000,
  });

  if (dashLoading || !dash) {
    return <div className="loading">加载仪表盘…</div>;
  }

  const passRatePct = (dash.pass_rate * 100).toFixed(1);

  return (
    <div className="page">
      {/* 统计卡片 */}
      <div className="stat-grid">
        <StatCard
          label="总 Run 数"
          value={dash.total_runs.toLocaleString()}
          sub={`今日新增 ${dash.runs_last_24h}`}
          color="#6366f1"
        />
        <StatCard
          label="整体通过率"
          value={`${passRatePct}%`}
          color={dash.pass_rate >= 0.8 ? "#10b981" : dash.pass_rate >= 0.5 ? "#f59e0b" : "#ef4444"}
        />
        <StatCard
          label="累计 API 成本"
          value={`$${dash.total_cost_usd.toFixed(2)}`}
          sub={`${dash.total_batches} 个批次`}
          color="#f59e0b"
        />
        <StatCard
          label="队列 / 活跃"
          value={`${dash.queue_depth} / ${dash.active_runs}`}
          sub="实时"
          color="#10b981"
        />
      </div>

      {/* 趋势图 */}
      <div className="chart-row">
        <TrendChart points={trend} />
        <CostChart points={trend} />
      </div>

      {/* 最近批次 */}
      <RecentBatches />
    </div>
  );
}
