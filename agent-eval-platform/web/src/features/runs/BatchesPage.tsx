// 批次列表 + 批次详情（运行中自动轮询，ch36 TanStack Query 模式）
import { useQuery } from "@tanstack/react-query";
import { Link, useParams } from "react-router-dom";
import { fetchBatches, fetchRuns } from "../../api/client";

export function BatchesPage() {
  const { data: batches, isLoading, error } = useQuery({
    queryKey: ["batches"],
    queryFn: fetchBatches,
    refetchInterval: (q) =>
      q.state.data?.some((b) => b.status === "running") ? 2000 : 15000,
  });

  if (isLoading) return <p className="empty">加载中…</p>;
  if (error) return <p className="empty">出错了：{String(error)}</p>;

  return (
    <main className="page">
      <h1>评测批次</h1>
      {batches?.length === 0 && (
        <p className="empty">
          还没有批次。试试：<code>./scripts/demo-batch.sh</code>
        </p>
      )}
      <table>
        <thead>
          <tr><th>名称</th><th>状态</th><th>通过</th><th>创建时间</th></tr>
        </thead>
        <tbody>
          {batches?.map((b) => (
            <tr key={b.id}>
              <td><Link to={`/batches/${b.id}`}>{b.name}</Link></td>
              <td><span className={`badge ${b.status}`}>{b.status}</span></td>
              <td>{b.passed} / {b.total}</td>
              <td>{new Date(b.created_at).toLocaleString()}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </main>
  );
}

export function BatchDetailPage() {
  const { batchId } = useParams();
  const { data: runs } = useQuery({
    queryKey: ["batches", batchId, "runs"],
    queryFn: () => fetchRuns(batchId!),
    refetchInterval: (q) =>
      q.state.data?.some((r) => ["queued", "leased", "running"].includes(r.status))
        ? 1500
        : false,
  });

  return (
    <main className="page">
      <h1>批次 {batchId?.slice(0, 8)}</h1>
      <table>
        <thead>
          <tr><th>Case</th><th>状态</th><th>分数</th><th>成本</th><th>轮数</th><th></th></tr>
        </thead>
        <tbody>
          {runs?.map((r) => (
            <tr key={r.id} className={r.status === "failed" || r.status === "error" ? "row-failed" : ""}>
              <td>{r.case_id}</td>
              <td><span className={`badge ${r.status}`}>{r.status}</span></td>
              <td>{r.score?.toFixed(2) ?? "—"}</td>
              <td>{r.cost_usd != null ? `$${r.cost_usd.toFixed(4)}` : "—"}</td>
              <td>{r.turns ?? "—"}</td>
              <td><Link to={`/runs/${r.id}`}>轨迹 →</Link></td>
            </tr>
          ))}
        </tbody>
      </table>
    </main>
  );
}
