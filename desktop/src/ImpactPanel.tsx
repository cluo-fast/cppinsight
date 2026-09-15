import React, { useEffect, useMemo, useState } from "react";
import { queryGraph } from "./api";
import { createGraphRequest } from "./graphWorkbenchState";
import { summarizeImpact } from "./impactAnalysis";
import type { GraphNode, GraphQueryResponse } from "./types";

export default function ImpactPanel({ root, projectId, target, generation, onClose, onLocate, onSource }: {
  root: string; projectId: number; target: GraphNode; generation: number;
  onClose: () => void; onLocate: (node: GraphNode) => void;
  onSource: (file: string, line: number) => void;
}) {
  const [depth, setDepth] = useState(3);
  const [result, setResult] = useState<GraphQueryResponse | null>(null);
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(true);
  useEffect(() => {
    let cancelled = false;
    setLoading(true); setResult(null); setError("");
    void queryGraph(root, createGraphRequest(projectId, "entry_paths", "incoming", depth, "", "all", "calls", null, [target.id]))
      .then(data => { if (!cancelled) setResult(data); })
      .catch(error => { if (!cancelled) setError(String(error)); })
      .finally(() => { if (!cancelled) setLoading(false); });
    return () => { cancelled = true; };
  }, [root, projectId, target.id, depth, generation]);
  const summary = useMemo(() => result ? summarizeImpact(result, target.id) : null, [result, target.id]);
  return <section className="impact-panel" aria-label="修改影响分析">
    <div className="graph-toolbar">
      <div><h3>改这里会影响哪里</h3><strong>{target.qualified_name || target.label}</strong></div>
      <button className="secondary" onClick={onClose}>关闭影响分析</button>
    </div>
    <label>向上追踪 <select aria-label="影响追踪深度" value={depth} onChange={event => setDepth(Number(event.target.value))}>
      {[1, 2, 3, 4, 5, 6, 7, 8].map(value => <option key={value} value={value}>{value} 层</option>)}
    </select></label>
    <p className="muted">依据本地索引的调用关系，最多 {depth} 层、500 个节点。列出每个调用者的一条最短证据链；潜在影响仍需结合代码和测试确认。</p>
    {loading ? <p role="status">正在追踪上游调用…</p> : null}
    {error ? <p role="alert">影响查询失败：{error}</p> : null}
    {summary ? <>
      {summary.partial ? <p className="graph-truncated" role="status">结果超过当前节点上限，仅展示返回节点中可验证的路径。请降低深度或选择更具体的函数；当前列表不是完整影响范围。</p> : null}
      {!summary.root ? <p role="status">当前结果未包含目标符号。可能因分页或索引变化，请缩小范围或重新索引后重试。</p> : <>
        <div className="graph-status-row">
          <span>直接调用者 {summary.direct.length}</span><span>间接调用者 {summary.indirect.length}</span>
          <span>入口候选 {summary.entries.length}</span><span>涉及文件 {summary.files.length}</span><span>涉及模块 {summary.modules.length}</span>
        </div>
        {summary.entries.length > 0 ? <p>可能受影响的入口：{summary.entries.map(item => item.node.qualified_name || item.node.label).join("、")}</p> : <p className="muted">在当前范围内未发现入口候选，可增加追踪深度。</p>}
        {summary.items.length === 0 ? <p>当前索引和深度内未发现上游调用；这不代表修改没有影响。</p> : null}
        <ul className="impact-list">{summary.items.map(item => <li key={item.node.id}>
          <button className="secondary" onClick={() => onLocate(item.node)}>{item.node.qualified_name || item.node.label}</button>
          <span>{item.distance === 1 ? "直接调用" : `间接调用 · ${item.distance} 层`}{item.node.kind === "entry" ? " · 入口候选" : ""}{item.uncertain ? " · 含推断或歧义" : ""}</span>
          <small>{item.node.file}:{item.node.line}</small>
          <details><summary>查看调用链与证据</summary>
            <p>{item.path.map(node => node.qualified_name || node.label).join(" → ")}</p>
            {item.edges.map(edge => <div key={edge.id}>
              <span>{edge.confidence}{edge.ambiguous ? " · 歧义" : ""} </span>
              {edge.evidence_ids.length === 0 ? <span>无源码证据</span> : edge.evidence_ids.map(id => {
                const evidence = result?.evidence.find(item => item.id === id);
                return evidence ? <button className="source-link" key={id} onClick={() => onSource(evidence.file, evidence.line)}>{evidence.file}:{evidence.line}</button> : <span key={id}>证据未返回 </span>;
              })}
            </div>)}
          </details>
        </li>)}</ul>
      </>}
    </> : null}
  </section>;
}
