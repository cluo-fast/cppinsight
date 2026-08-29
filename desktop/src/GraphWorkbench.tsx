import React, { useEffect, useMemo, useRef, useState } from "react";
import cytoscape from "cytoscape";
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
  onInspectSymbol?: (query: string) => void;
  onAskAi?: (question: string) => void;
  onGraphSnapshot?: (svg: string) => void;
};

function createRequest(
  projectId: number,
  graphKind: GraphKind,
  direction: GraphDirection,
  depth: number,
  search: string,
  confidence: GraphConfidence | "all",
  relationFilter: string,
  continuation: string | null,
  roots: string[] = [],
  pathTarget: string | null = null
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
      relation_kinds: relationFilter.trim()
        ? relationFilter.split(",").map((item) => item.trim()).filter(Boolean)
        : [],
      confidence: confidence === "all" ? [] : [confidence],
      search: search.trim() || null
    },
    max_nodes: 500,
    path_target: pathTarget,
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

function mergeGraphResponses(previous: GraphQueryResponse, next: GraphQueryResponse): GraphQueryResponse {
  const nodes = new Map(previous.nodes.map((node) => [node.id, node]));
  next.nodes.forEach((node) => nodes.set(node.id, node));
  const edges = new Map(previous.edges.map((edge) => [edge.id, edge]));
  next.edges.forEach((edge) => edges.set(edge.id, edge));
  const evidence = new Map(previous.evidence.map((item) => [item.id, item]));
  next.evidence.forEach((item) => evidence.set(item.id, item));
  return {
    ...next,
    nodes: [...nodes.values()],
    edges: [...edges.values()],
    evidence: [...evidence.values()]
  };
}

export default function GraphWorkbench({ root, projectId, onStatus, onInspectSymbol, onAskAi, onGraphSnapshot }: Props) {
  const [graphKind, setGraphKind] = useState<GraphKind>("module_dependencies");
  const [direction, setDirection] = useState<GraphDirection>("both");
  const [depth, setDepth] = useState(2);
  const [search, setSearch] = useState("");
  const [confidence, setConfidence] = useState<GraphConfidence | "all">("all");
  const [relationFilter, setRelationFilter] = useState("");
  const [rootIds, setRootIds] = useState<string[]>([]);
  const [pathTarget, setPathTarget] = useState<string | null>(null);
  const [zoom, setZoom] = useState(1);
  const [response, setResponse] = useState<GraphQueryResponse | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [positions, setPositions] = useState<Record<string, GraphPosition>>({});
  const [draggingId, setDraggingId] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const canvasRef = useRef<SVGSVGElement | null>(null);
  const cytoscapeContainerRef = useRef<HTMLDivElement | null>(null);
  const cytoscapeRef = useRef<any>(null);
  const [cytoscapeReady] = useState(() => typeof cytoscape === "function");

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

  useEffect(() => {
    if (!cytoscapeContainerRef.current || nodes.length === 0) {
      return undefined;
    }
    const nodeElements = nodes.map((node) => ({
      data: { id: node.id, label: shortLabel(node), color: nodeColor(node) },
      position: positions[node.id] ?? { x: 80, y: 80 }
    }));
    const edgeElements = edges.map((edge) => ({
      data: { id: edge.id, source: edge.source, target: edge.target, label: edge.kind }
    }));
    const layoutName = graphKind === "module_dependencies" || graphKind === "file_includes" || graphKind === "entry_paths"
      ? "breadthfirst"
      : "cose";
    const cy = cytoscape({
      container: cytoscapeContainerRef.current,
      elements: { nodes: nodeElements, edges: edgeElements },
      boxSelectionEnabled: true,
      selectionType: "additive",
      userPanningEnabled: true,
      style: [
        {
          selector: "node",
          style: {
            label: "data(label)",
            "background-color": "data(color)",
            color: "#e8eef5",
            "font-size": 11,
            "text-valign": "bottom",
            "text-margin-y": 8,
            "border-width": 1,
            "border-color": "#dbeafe"
          }
        },
        {
          selector: "edge",
          style: {
            width: 2,
            "line-color": "#7890a8",
            "target-arrow-color": "#7890a8",
            "target-arrow-shape": "triangle",
            "curve-style": "bezier"
          }
        },
        { selector: "node.central", style: { width: 34, height: 34, "border-width": 3 } },
        {
          selector: ".path",
          style: { "border-width": 3, "border-color": "#fff3bf", "line-color": "#ffd166", width: 4 }
        }
      ],
      layout: { name: layoutName, fit: true, padding: 40, directed: true, spacingFactor: 1.1 }
    });
    cytoscapeRef.current = cy;
    cy.on("tap", "node", (event: any) => setSelectedId(event.target.id()));
    cy.nodes().forEach((node: any) => {
      if (pathNodes.has(node.id())) node.addClass("path");
      const source = nodes.find((item) => item.id === node.id());
      if (source && centralNodes.has(source.id)) node.addClass("central");
    });
    cy.edges().forEach((edge: any) => {
      if (pathNodes.has(edge.source().id()) && pathNodes.has(edge.target().id())) edge.addClass("path");
    });
    cy.on("dragfree", "node", () => {
      const next: Record<string, GraphPosition> = {};
      cy.nodes().forEach((node: any) => {
        const position = node.position();
        next[node.id()] = { x: position.x, y: position.y };
      });
      setPositions(next);
    });
    return () => {
      cy.destroy();
      cytoscapeRef.current = null;
    };
  }, [nodes, edges, pathNodes, centralNodes, graphKind]);

  useEffect(() => {
    const cy = cytoscapeRef.current;
    if (!cy) return;
    cy.nodes().forEach((node: any) => {
      const position = positions[node.id()];
      if (position) node.position(position);
    });
    cy.zoom(zoom);
  }, [positions, zoom]);

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
        createRequest(
          projectId,
          graphKind,
          direction,
          depth,
          search,
          confidence,
          relationFilter,
          nextContinuation,
          rootIds,
          pathTarget
        )
      );
      const merged = nextContinuation && response ? mergeGraphResponses(response, next) : next;
      setResponse(merged);
      setSelectedId((current) =>
        current && merged.nodes.some((node) => node.id === current) ? current : merged.nodes[0]?.id ?? null
      );
      const viewKey = `${direction}:${depth}:${search.trim()}:${confidence}:${relationFilter}:${rootIds.join(",")}:${pathTarget ?? ""}`;
      let saved: SavedGraphLayout | null = null;
      try {
        saved = await loadGraphLayout(root, projectId, graphKind, viewKey);
      } catch {
        // Layout storage is best-effort; graph data remains usable when upgrading an old index.
      }
      layoutFor(merged.nodes, saved?.positions);
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
    const viewKey = `${direction}:${depth}:${search.trim()}:${confidence}:${relationFilter}:${rootIds.join(",")}:${pathTarget ?? ""}`;
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

  function downloadBlob(blob: Blob, filename: string) {
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement("a");
    anchor.href = url;
    anchor.download = filename;
    anchor.click();
    URL.revokeObjectURL(url);
  }

  function downloadDataUrl(dataUrl: string, filename: string) {
    const anchor = document.createElement("a");
    anchor.href = dataUrl;
    anchor.download = filename;
    anchor.click();
  }

  function graphSvgSnapshot(): string {
    const escapeXml = (value: string) => value.replace(/[<>&'\"]/g, (character) => ({
      "<": "&lt;", ">": "&gt;", "&": "&amp;", "'": "&apos;", "\"": "&quot;"
    }[character] ?? character));
    const lines = edges.map((edge) => {
      const source = positions[edge.source];
      const target = positions[edge.target];
      if (!source || !target) return "";
      const highlighted = pathNodes.has(edge.source) && pathNodes.has(edge.target);
      return `<line x1="${source.x}" y1="${source.y}" x2="${target.x}" y2="${target.y}" stroke="${highlighted ? "#ffd166" : "#7890a8"}" stroke-width="${Math.min(6, 1 + edge.weight / 2)}" marker-end="url(#arrow)"/>`;
    }).join("");
    const circles = nodes.map((node) => {
      const position = positions[node.id] ?? { x: 80, y: 80 };
      const radius = centralNodes.has(node.id) ? 17 : 13;
      return `<circle cx="${position.x}" cy="${position.y}" r="${radius}" fill="${nodeColor(node)}"/><text x="${position.x}" y="${position.y + 30}" text-anchor="middle" fill="#e8eef5" font-size="11">${escapeXml(shortLabel(node))}</text>`;
    }).join("");
    return `<svg xmlns="http://www.w3.org/2000/svg" width="${CANVAS_WIDTH}" height="${CANVAS_HEIGHT}" viewBox="0 0 ${CANVAS_WIDTH} ${CANVAS_HEIGHT}"><defs><marker id="arrow" markerWidth="8" markerHeight="8" refX="7" refY="4" orient="auto"><path d="M0,0 L8,4 L0,8 z" fill="#7890a8"/></marker></defs><rect width="100%" height="100%" fill="#0d141b"/>${lines}${circles}</svg>`;
  }

  useEffect(() => {
    onGraphSnapshot?.(nodes.length > 0 ? graphSvgSnapshot() : "");
  }, [nodes, edges, positions, pathNodes, centralNodes, onGraphSnapshot]);

  function exportSvg() {
    const serialized = graphSvgSnapshot();
    downloadBlob(new Blob([serialized], { type: "image/svg+xml;charset=utf-8" }), `cppinsight-${graphKind}.svg`);
  }

  function exportPng() {
    const cy = cytoscapeRef.current;
    if (cy) {
      downloadDataUrl(cy.png({ bg: "#0d141b", full: true, scale: 2 }), `cppinsight-${graphKind}.png`);
      return;
    }
    setError("Cytoscape 画布尚未初始化");
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
          <button className="secondary" disabled={nodes.length === 0} onClick={exportSvg}>
            导出 SVG
          </button>
          <button className="secondary" disabled={nodes.length === 0} onClick={exportPng}>
            导出 PNG
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
        <label>
          <span>置信度</span>
          <select value={confidence} onChange={(event) => setConfidence(event.target.value as GraphConfidence | "all")}>
            <option value="all">全部</option>
            <option value="resolved">已解析</option>
            <option value="precise">编译数据库</option>
            <option value="heuristic">启发式</option>
            <option value="ambiguous">歧义</option>
            <option value="unresolved">未解析</option>
          </select>
        </label>
        <label>
          <span>关系类型</span>
          <input
            placeholder="calls, inherits…"
            value={relationFilter}
            onChange={(event) => setRelationFilter(event.target.value)}
          />
        </label>
        <button className="secondary" disabled={loading} onClick={() => void loadGraph()}>
          应用筛选
        </button>
        <button className="secondary" disabled={zoom >= 2} onClick={() => setZoom((value) => Math.min(2, value + 0.2))}>放大</button>
        <button className="secondary" disabled={zoom <= 0.6} onClick={() => setZoom((value) => Math.max(0.6, value - 0.2))}>缩小</button>
        <button className="secondary" onClick={() => setZoom(1)}>适应</button>
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
          ) : cytoscapeReady ? (
            <div
              ref={cytoscapeContainerRef}
              className="graph-canvas cytoscape-canvas"
              data-engine="cytoscape.js"
              aria-label="Cytoscape.js 代码关系图"
            />
          ) : (
            <svg
              ref={canvasRef}
              className="graph-canvas"
              viewBox={`0 0 ${CANVAS_WIDTH / zoom} ${CANVAS_HEIGHT / zoom}`}
              role="img"
              aria-label="代码关系图"
              onPointerMove={onPointerMove}
              onPointerUp={() => setDraggingId(null)}
              onPointerLeave={() => setDraggingId(null)}
              onWheel={(event) => {
                event.preventDefault();
                setZoom((value) => Math.max(0.6, Math.min(2, value + (event.deltaY < 0 ? 0.1 : -0.1))));
              }}
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
          {nodes.length > 0 ? (
            <svg className="graph-minimap" viewBox={`0 0 ${CANVAS_WIDTH} ${CANVAS_HEIGHT}`} role="img" aria-label="关系图小地图">
              <rect width={CANVAS_WIDTH} height={CANVAS_HEIGHT} fill="#0d141b" />
              {edges.map((edge) => {
                const source = positions[edge.source];
                const target = positions[edge.target];
                return source && target ? <line key={edge.id} x1={source.x} y1={source.y} x2={target.x} y2={target.y} stroke="#7890a8" strokeWidth="3" /> : null;
              })}
              {nodes.map((node) => {
                const position = positions[node.id] ?? { x: 80, y: 80 };
                return <circle key={node.id} cx={position.x} cy={position.y} r="8" fill={nodeColor(node)} />;
              })}
            </svg>
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
              {(selected.kind === "function" || selected.kind === "class" || selected.kind === "struct" || selected.kind === "symbol") && selected.qualified_name ? (
                <div className="graph-inspector-actions">
                  <button className="secondary" onClick={() => onInspectSymbol?.(selected.qualified_name)}>
                    查影响面
                  </button>
                  <button className="secondary" onClick={() => onAskAi?.(`请解释 ${selected.qualified_name} 的职责、调用关系和潜在影响面。`)}>
                    问 AI
                  </button>
                </div>
              ) : null}
              <div className="graph-node-actions">
                <button
                  className="secondary"
                  onClick={() => setRootIds([selected.id])}
                >
                  设为查询根
                </button>
                <button
                  className="secondary"
                  onClick={() => setPathTarget(selected.id)}
                >
                  设为路径目标
                </button>
              </div>
              {rootIds.length > 0 || pathTarget ? (
                <p className="muted graph-query-hint">
                  根：{rootIds.join(", ") || "自动"} · 目标：{pathTarget || "未设置"}（点击“应用筛选”执行）
                </p>
              ) : null}
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
