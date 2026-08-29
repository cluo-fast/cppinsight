import React, { useEffect, useMemo, useRef, useState } from "react";
import {
  loadGraphLayout,
  queryGraph,
  saveGraphLayout
} from "./api";
import type {
  GraphConfidence,
  GraphDirection,
  GraphEdge,
  GraphKind,
  GraphNode,
  GraphPosition,
  GraphQueryRequest,
  GraphQueryResponse,
  SavedGraphLayout
} from "./types";

const GRAPH_KINDS: { value: GraphKind; label: string }[] = [
  { value: "module_dependencies", label: "模块依赖" },
  { value: "function_calls", label: "函数调用" },
  { value: "class_relations", label: "类关系" },
  { value: "file_includes", label: "文件包含" },
  { value: "entry_paths", label: "入口路径" }
];

const DIRECTIONS: { value: GraphDirection; label: string }[] = [
  { value: "both", label: "双向" },
  { value: "outgoing", label: "向下游" },
  { value: "incoming", label: "向上游" }
];

const CANVAS_WIDTH = 1000;
const CANVAS_HEIGHT = 620;

type Props = {
  root: string;
  projectId: number;
  onStatus?: (message: string | null) => void;
};

function createRequest(
  projectId: number,
  graphKind: GraphKind,
  direction: GraphDirection,
  depth: number,
  search: string,
  continuation: string | null,
  roots: string[] = []
): GraphQueryRequest {
  return {
    protocol_version: 1,
    project_id: projectId,
    graph_kind: graphKind,
    root_node_ids: roots,
    direction,
    depth,
    filters: {
      module_ids: [],
      file_paths: [],
      symbol_kinds: [],
      relation_kinds: [],
      confidence: [] as GraphConfidence[],
      search: search.trim() || null
    },
    max_nodes: 500,
    path_target: null,
    continuation
  };
}

function fallbackPosition(index: number, total: number): GraphPosition {
  const columns = Math.max(1, Math.ceil(Math.sqrt(total)));
  const rows = Math.max(1, Math.ceil(total / columns));
  const column = index % columns;
  const row = Math.floor(index / columns);
  return {
    x: 80 + (column / Math.max(1, columns - 1)) * 840,
    y: 80 + (row / Math.max(1, rows - 1)) * 460
  };
}

function nodeColor(node: GraphNode): string {
  switch (node.kind) {
    case "module":
      return "#54a7ff";
    case "file":
      return "#63d7b0";
    case "class":
    case "struct":
      return "#d3a6ff";
    case "function":
      return "#ffc857";
    case "unresolved":
      return "#ff8e8e";
    default:
      return "#b8c7d9";
  }
}

function shortLabel(node: GraphNode): string {
  const value = node.label || node.qualified_name || node.id;
  return value.length > 24 ? `${value.slice(0, 23)}…` : value;
}

function edgeKey(edge: GraphEdge): string {
  return `${edge.source}->${edge.target}`;
}

export default function GraphWorkbench({ root, projectId, onStatus }: Props) {
  const [graphKind, setGraphKind] = useState<GraphKind>("module_dependencies");
  const [direction, setDirection] = useState<GraphDirection>("both");
  const [depth, setDepth] = useState(2);
  const [search, setSearch] = useState("");
  const [response, setResponse] = useState<GraphQueryResponse | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [positions, setPositions] = useState<Record<string, GraphPosition>>({});
  const [draggingId, setDraggingId] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const canvasRef = useRef<SVGSVGElement | null>(null);

  const nodes = response?.nodes ?? [];
  const edges = response?.edges ?? [];
  const selected = nodes.find((node) => node.id === selectedId) ?? null;
  const pathNodes = useMemo(
    () => new Set(response?.statistics.shortest_path_node_ids ?? []),
    [response]
  );
  const centralNodes = useMemo(
    () => new Set(response?.statistics.central_node_ids ?? []),
    [response]
  );

  function layoutFor(nodesToLayout: GraphNode[], saved?: Record<string, GraphPosition>) {
    const next: Record<string, GraphPosition> = {};
    nodesToLayout.forEach((node, index) => {
      next[node.id] = saved?.[node.id] ?? fallbackPosition(index, nodesToLayout.length);
    });
    setPositions(next);
  }

  async function loadGraph(nextContinuation: string | null = null) {
    setLoading(true);
    setError(null);
    onStatus?.("正在查询关系图…");
    try {
      const next = await queryGraph(
        root,
        createRequest(projectId, graphKind, direction, depth, search, nextContinuation)
      );
      setResponse(next);
      setSelectedId((current) =>
        current && next.nodes.some((node) => node.id === current) ? current : next.nodes[0]?.id ?? null
      );
      const viewKey = `${direction}:${depth}:${search.trim()}`;
      let saved: SavedGraphLayout | null = null;
      try {
        saved = await loadGraphLayout(root, projectId, graphKind, viewKey);
      } catch {
        // Layout storage is best-effort; graph data remains usable when upgrading an old index.
      }
      layoutFor(next.nodes, saved?.positions);
      onStatus?.(null);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setError(message);
      onStatus?.(null);
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    void loadGraph();
    // The explicit controls below are the intended query boundaries.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [root, projectId, graphKind]);

  function svgPoint(event: React.PointerEvent<SVGSVGElement>): GraphPosition {
    const svg = canvasRef.current;
    if (!svg) {
      return { x: 0, y: 0 };
    }
    const rect = svg.getBoundingClientRect();
    return {
      x: ((event.clientX - rect.left) / rect.width) * CANVAS_WIDTH,
      y: ((event.clientY - rect.top) / rect.height) * CANVAS_HEIGHT
    };
  }

  function saveCurrentLayout() {
    const viewKey = `${direction}:${depth}:${search.trim()}`;
    const layout: SavedGraphLayout = {
      protocol_version: 1,
      project_id: projectId,
      graph_kind: graphKind,
      view_key: viewKey,
      positions,
      updated_at: ""
    };
    void saveGraphLayout(root, layout)
      .then(() => onStatus?.("布局已保存"))
      .catch((err) => setError(err instanceof Error ? err.message : String(err)));
  }

  function onPointerMove(event: React.PointerEvent<SVGSVGElement>) {
    if (!draggingId) {
      return;
    }
    const point = svgPoint(event);
    setPositions((current) => ({ ...current, [draggingId]: point }));
  }

  const selectedEdges = selected
    ? edges.filter((edge) => edge.source === selected.id || edge.target === selected.id)
    : [];

  return (
    <section className="graph-workbench panel">
      <div className="graph-toolbar">
        <div>
          <h2>关系图工作台</h2>
          <p className="muted">统一图协议 · 本地索引 · 可追溯证据</p>
        </div>
        <div className="graph-toolbar-actions">
          <button className="secondary" disabled={loading} onClick={() => void loadGraph()}>
            {loading ? "查询中…" : "刷新"}
          </button>
          <button className="secondary" disabled={nodes.length === 0} onClick={saveCurrentLayout}>
            保存布局
          </button>
        </div>
      </div>

      <div className="graph-controls">
        <label>
          <span>图类型</span>
          <select value={graphKind} onChange={(event) => setGraphKind(event.target.value as GraphKind)}>
            {GRAPH_KINDS.map((kind) => (
              <option key={kind.value} value={kind.value}>
                {kind.label}
              </option>
            ))}
          </select>
        </label>
        <label>
          <span>方向</span>
          <select value={direction} onChange={(event) => setDirection(event.target.value as GraphDirection)}>
            {DIRECTIONS.map((item) => (
              <option key={item.value} value={item.value}>
                {item.label}
              </option>
            ))}
          </select>
        </label>
        <label>
          <span>深度</span>
          <input type="number" min={0} max={8} value={depth} onChange={(event) => setDepth(Number(event.target.value))} />
        </label>
        <label className="graph-search">
          <span>搜索</span>
          <input
            type="search"
            placeholder="函数、类、文件…"
            value={search}
            onChange={(event) => setSearch(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                void loadGraph();
              }
            }}
          />
        </label>
        <button className="secondary" disabled={loading} onClick={() => void loadGraph()}>
          应用筛选
        </button>
      </div>

      {error ? <div className="banner error">关系图查询失败：{error}</div> : null}
      {response ? (
        <div className="graph-status-row">
          <span>{response.statistics.returned_node_count} / {response.statistics.original_node_count} 节点</span>
          <span>{response.statistics.returned_edge_count} / {response.statistics.original_edge_count} 条关系</span>
          <span>置信度：{response.confidence.overall}</span>
          <span>循环：{response.statistics.cycle_count}</span>
          <span>关键节点：{response.statistics.central_node_ids.length}</span>
          {response.truncated ? <span className="graph-truncated">已截断</span> : null}
        </div>
      ) : null}

      <div className="graph-layout">
        <div className="graph-canvas-wrap">
          {nodes.length === 0 ? (
            <div className="graph-empty muted">暂无关系数据。请先完成索引，或调整图类型与筛选条件。</div>
          ) : (
            <svg
              ref={canvasRef}
              className="graph-canvas"
              viewBox={`0 0 ${CANVAS_WIDTH} ${CANVAS_HEIGHT}`}
              role="img"
              aria-label="代码关系图"
              onPointerMove={onPointerMove}
              onPointerUp={() => setDraggingId(null)}
              onPointerLeave={() => setDraggingId(null)}
            >
              <defs>
                <marker id="graph-arrow" markerWidth="8" markerHeight="8" refX="7" refY="4" orient="auto">
                  <path d="M0,0 L8,4 L0,8 z" fill="#7890a8" />
                </marker>
              </defs>
              <rect width={CANVAS_WIDTH} height={CANVAS_HEIGHT} fill="transparent" />
              {edges.map((edge) => {
                const source = positions[edge.source];
                const target = positions[edge.target];
                if (!source || !target) return null;
                const highlighted = pathNodes.has(edge.source) && pathNodes.has(edge.target);
                return (
                  <line
                    key={edge.id}
                    className={`graph-edge${highlighted ? " highlighted" : ""}`}
                    x1={source.x}
                    y1={source.y}
                    x2={target.x}
                    y2={target.y}
                    strokeWidth={Math.min(6, 1 + edge.weight / 2)}
                    markerEnd="url(#graph-arrow)"
                  />
                );
              })}
              {nodes.map((node) => {
                const position = positions[node.id] ?? { x: 80, y: 80 };
                const highlighted = pathNodes.has(node.id);
                const central = centralNodes.has(node.id);
                return (
                  <g
                    key={node.id}
                    className={`graph-node${selectedId === node.id ? " selected" : ""}${highlighted ? " path" : ""}`}
                    transform={`translate(${position.x},${position.y})`}
                    onPointerDown={(event) => {
                      event.stopPropagation();
                      setSelectedId(node.id);
                      setDraggingId(node.id);
                      event.currentTarget.setPointerCapture(event.pointerId);
                    }}
                    onClick={() => setSelectedId(node.id)}
                  >
                    <circle r={central ? 17 : 13} fill={nodeColor(node)} />
                    <text y={30} textAnchor="middle">{shortLabel(node)}</text>
                  </g>
                );
              })}
            </svg>
          )}
          {response?.truncated ? (
            <button className="graph-more" onClick={() => void loadGraph(response.continuation)}>
              加载下一页（{response.continuation}）
            </button>
          ) : null}
        </div>

        <aside className="graph-inspector">
          <h3>{selected ? "节点详情" : "关系说明"}</h3>
          {selected ? (
            <>
              <div className="graph-node-title">
                <span className="graph-node-dot" style={{ background: nodeColor(selected) }} />
                <strong>{selected.label}</strong>
              </div>
              <dl className="graph-details">
                <dt>稳定 ID</dt><dd><code>{selected.id}</code></dd>
                <dt>限定名</dt><dd>{selected.qualified_name || "—"}</dd>
                <dt>位置</dt><dd>{selected.file ? `${selected.file}:${selected.line}` : "—"}</dd>
                <dt>置信度</dt><dd>{selected.confidence}{selected.ambiguous ? " · 歧义" : ""}</dd>
              </dl>
              <h4>关系证据</h4>
              {selectedEdges.length === 0 ? <p className="muted">暂无相邻关系</p> : (
                <ul className="graph-evidence-list">
                  {selectedEdges.slice(0, 12).map((edge) => (
                    <li key={edge.id}>
                      <span>{edge.kind} · {edge.source === selected.id ? "→" : "←"}</span>
                      <small>{edge.confidence} · {edge.evidence_ids.length} 条证据</small>
                      {response?.evidence.filter((item) => edge.evidence_ids.includes(item.id)).slice(0, 3).map((item) => (
                        <code key={item.id}>{item.file}:{item.line}</code>
                      ))}
                    </li>
                  ))}
                </ul>
              )}
            </>
          ) : (
            <>
              <p className="muted">拖拽节点调整布局，点击节点查看源码位置、置信度与证据。</p>
              <ul className="graph-legend">
                <li><span className="legend-dot" style={{ background: "#54a7ff" }} />模块</li>
                <li><span className="legend-dot" style={{ background: "#63d7b0" }} />文件</li>
                <li><span className="legend-dot" style={{ background: "#ffc857" }} />函数</li>
                <li><span className="legend-dot" style={{ background: "#d3a6ff" }} />类型</li>
                <li><span className="legend-dot" style={{ background: "#ff8e8e" }} />未解析</li>
              </ul>
            </>
          )}
        </aside>
      </div>
    </section>
  );
}
