import React, { useEffect, useMemo, useRef, useState } from "react";
import {
  loadGraphLayout,
  queryGraph,
  saveGraphLayout
} from "./api";
import {
  GRAPH_CANVAS_HEIGHT,
  GRAPH_CANVAS_WIDTH,
  graphSvgSnapshot as renderGraphSvgSnapshot,
  hiddenDescendants,
  nodeColor,
  shortLabel
} from "./graphSnapshot";
import { createGraphRequest, mergeGraphResponses, reconcileGraphScope } from "./graphWorkbenchState";
import { pathEdgeIds } from "./impactAnalysis";
import ImpactPanel from "./ImpactPanel";
import SourcePreviewPanel from "./SourcePreviewPanel";
import type {
  GraphConfidence,
  GraphDirection,
  GraphKind,
  GraphNode,
  GraphPosition,
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

const CANVAS_WIDTH = GRAPH_CANVAS_WIDTH;
const CANVAS_HEIGHT = GRAPH_CANVAS_HEIGHT;

type Props = {
  root: string;
  projectId: number;
  onStatus?: (message: string | null) => void;
  onInspectSymbol?: (query: string) => void;
  onAskAi?: (question: string, nodeId?: string) => void;
  onGraphSnapshot?: (snapshot: { svg: string; view: string }) => void;
  requestedGraphKind?: GraphKind | null;
  requestedGraphNodeId?: string | null;
  graphRequestId?: number;
  graphDataVersion?: number;
};

type CytoscapeFactory = typeof import("cytoscape");

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

export default function GraphWorkbench({ root, projectId, onStatus, onInspectSymbol, onAskAi, onGraphSnapshot, requestedGraphKind, requestedGraphNodeId, graphRequestId = 0, graphDataVersion = 0 }: Props) {
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
  const [responseDirection, setResponseDirection] = useState<GraphDirection>("both");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [positions, setPositions] = useState<Record<string, GraphPosition>>({});
  const [collapsedIds, setCollapsedIds] = useState<Set<string>>(new Set());
  const [activeView, setActiveView] = useState("");
  const [draggingId, setDraggingId] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [impactTarget, setImpactTarget] = useState<GraphNode | null>(null);
  const [sourceLocation, setSourceLocation] = useState<{ file: string; line: number } | null>(null);
  const canvasRef = useRef<SVGSVGElement | null>(null);
  const cytoscapeContainerRef = useRef<HTMLDivElement | null>(null);
  const cytoscapeRef = useRef<any>(null);
  const positionsRef = useRef<Record<string, GraphPosition>>({});
  positionsRef.current = positions;
  const [cytoscapeFactory, setCytoscapeFactory] = useState<CytoscapeFactory | null>(null);
  const cytoscapeReady = cytoscapeFactory !== null;
  const consumedGraphKindRef = useRef<GraphKind | null>(null);
  const consumedGraphNodeRef = useRef<string | null>(null);
  const consumedGraphKindRequestRef = useRef<number | null>(null);
  const consumedGraphNodeRequestRef = useRef<number | null>(null);
  const graphQuerySequenceRef = useRef(0);
  const graphScopeRef = useRef({ root, projectId });

  const nodes = response?.nodes ?? [];
  const edges = response?.edges ?? [];
  const hiddenNodeIds = useMemo(() => hiddenDescendants(edges, collapsedIds), [edges, collapsedIds]);
  const visibleNodes = useMemo(() => nodes.filter((node) => !hiddenNodeIds.has(node.id)), [nodes, hiddenNodeIds]);
  const visibleEdges = useMemo(
    () => edges.filter((edge) => !hiddenNodeIds.has(edge.source) && !hiddenNodeIds.has(edge.target)),
    [edges, hiddenNodeIds]
  );
  const selected = visibleNodes.find((node) => node.id === selectedId) ?? null;
  const pathNodes = useMemo(
    () => new Set(response?.statistics.shortest_path_node_ids ?? []),
    [response]
  );
  const centralNodes = useMemo(
    () => new Set(response?.statistics.central_node_ids ?? []),
    [response]
  );
  const highlightedEdges = useMemo(() => pathEdgeIds(edges, response?.statistics.shortest_path_node_ids ?? [], responseDirection), [edges, response, responseDirection]);

  useEffect(() => {
    setImpactTarget(null);
    setSourceLocation(null);
  }, [root, projectId, graphDataVersion, graphRequestId]);

  function showSource(file: string, line: number) {
    setSourceLocation({ file, line });
  }

  function investigate(node: GraphNode, task: "impact" | "callers" | "callees" | "nearby") {
    if (task === "impact") setImpactTarget(node);
    setGraphKind(node.kind === "entry" ? "entry_paths" : "function_calls");
    setDirection(task === "callees" ? "outgoing" : task === "nearby" ? "both" : "incoming");
    setDepth(task === "impact" ? 3 : 1);
    setSearch(""); setConfidence("all"); setRelationFilter("calls");
    setPathTarget(null);
    setRootIds([node.id]);
  }

  useEffect(() => {
    if (nodes.length === 0 || cytoscapeFactory) {
      return undefined;
    }
    let cancelled = false;
    void import("cytoscape").then((module) => {
      if (!cancelled) {
        const loaded = module as unknown as { default?: CytoscapeFactory } & CytoscapeFactory;
        const factory = loaded.default ?? loaded;
        if (typeof factory !== "function") {
          throw new Error("Cytoscape module did not expose a callable renderer");
        }
        setCytoscapeFactory(() => factory);
      }
    }).catch(() => {
      // The deterministic SVG canvas remains usable when the optional renderer
      // cannot be loaded in a source-only development environment.
    });
    return () => {
      cancelled = true;
    };
  }, [nodes.length, cytoscapeFactory]);

  useEffect(() => {
    if (selectedId && hiddenNodeIds.has(selectedId)) setSelectedId(null);
  }, [hiddenNodeIds, selectedId]);

  useEffect(() => {
    const scope = reconcileGraphScope(graphScopeRef.current, { root, projectId });
    if (scope.changed) {
      graphScopeRef.current = scope.scope;
      // A navigation request from the previous project must not be replayed
      // against the new index. Keep the current view kind, but drop its scope.
      consumedGraphKindRef.current = requestedGraphKind ?? graphKind;
      consumedGraphNodeRef.current = requestedGraphNodeId ?? null;
      consumedGraphKindRequestRef.current = graphRequestId;
      consumedGraphNodeRequestRef.current = graphRequestId;
      setRootIds([]);
      setPathTarget(null);
    }
    graphQuerySequenceRef.current += 1;
    setResponse(null);
    setSelectedId(null);
    setPositions({});
    setCollapsedIds(new Set());
  }, [root, projectId, graphDataVersion, requestedGraphKind, requestedGraphNodeId, graphRequestId, graphKind]);

  useEffect(() => {
    const isNewRequest = graphRequestId !== consumedGraphKindRequestRef.current;
    consumedGraphKindRequestRef.current = graphRequestId;
    if (requestedGraphKind && (isNewRequest || requestedGraphKind !== consumedGraphKindRef.current)) {
      consumedGraphKindRef.current = requestedGraphKind;
      setGraphKind(requestedGraphKind);
    }
  }, [requestedGraphKind, graphRequestId]);

  useEffect(() => {
    const isNewRequest = graphRequestId !== consumedGraphNodeRequestRef.current;
    consumedGraphNodeRequestRef.current = graphRequestId;
    if (!requestedGraphNodeId) {
      consumedGraphNodeRef.current = null;
      if (rootIds.length > 0) setRootIds([]);
      setPathTarget(null);
      return;
    }
    if (isNewRequest || requestedGraphNodeId !== consumedGraphNodeRef.current) {
      consumedGraphNodeRef.current = requestedGraphNodeId;
      setGraphKind("function_calls");
      setRootIds([requestedGraphNodeId]);
      setPathTarget(null);
    }
  }, [requestedGraphNodeId, graphRequestId]);

  useEffect(() => {
    if (!cytoscapeContainerRef.current || nodes.length === 0 || !cytoscapeFactory) {
      return undefined;
    }
    const savedPositions = positionsRef.current;
    const hasCompletePositions = visibleNodes.every((node) => Boolean(savedPositions[node.id]));
    const nodeElements = visibleNodes.map((node) => ({
      data: { id: node.id, label: shortLabel(node), color: nodeColor(node) },
      ...(hasCompletePositions && savedPositions[node.id] ? { position: savedPositions[node.id] } : {})
    }));
    const edgeElements = visibleEdges.map((edge) => ({
      data: { id: edge.id, source: edge.source, target: edge.target, label: edge.kind }
    }));
    const layoutName = graphKind === "module_dependencies" || graphKind === "file_includes" || graphKind === "entry_paths"
      ? "breadthfirst"
      : "cose";
    const cy = cytoscapeFactory({
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
      ]
    });
    cytoscapeRef.current = cy;
    cy.on("tap", "node", (event: any) => setSelectedId(event.target.id()));
    cy.nodes().forEach((node: any) => {
      if (pathNodes.has(node.id())) node.addClass("path");
      const source = nodes.find((item) => item.id === node.id());
      if (source && centralNodes.has(source.id)) node.addClass("central");
    });
    cy.edges().forEach((edge: any) => {
      if (highlightedEdges.has(edge.id())) edge.addClass("path");
    });
    const syncCytoscapePositions = () => {
      const next: Record<string, GraphPosition> = {};
      cy.nodes().forEach((node: any) => {
        const position = node.position();
        next[node.id()] = { x: position.x, y: position.y };
      });
      setPositions((current) => {
      const unchanged = visibleNodes.every((node) => {
          const before = current[node.id];
          const after = next[node.id];
          return before && after && Math.abs(before.x - after.x) < 0.01 && Math.abs(before.y - after.y) < 0.01;
        });
        return unchanged ? current : next;
      });
    };
    cy.on("layoutstop", syncCytoscapePositions);
    cy.layout(hasCompletePositions
      ? { name: "preset", fit: true, padding: 40 }
      : { name: layoutName, fit: true, padding: 40, directed: true, spacingFactor: 1.1 }).run();
    cy.on("dragfree", "node", () => {
      syncCytoscapePositions();
    });
    return () => {
      cy.destroy();
      cytoscapeRef.current = null;
    };
  }, [nodes, visibleNodes, visibleEdges, pathNodes, highlightedEdges, centralNodes, graphKind, cytoscapeFactory]);

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
    const hasCompleteSavedLayout = nodesToLayout.length > 0 && nodesToLayout.every((node) => Boolean(saved?.[node.id]));
    if (cytoscapeReady && !hasCompleteSavedLayout) {
      setPositions({});
      return;
    }
    const next: Record<string, GraphPosition> = {};
    nodesToLayout.forEach((node, index) => {
      next[node.id] = saved?.[node.id] ?? fallbackPosition(index, nodesToLayout.length);
    });
    setPositions(next);
  }

  function changeGraphKind(nextKind: GraphKind) {
    setGraphKind(nextKind);
    setRootIds([]);
    setPathTarget(null);
    setResponse(null);
    setSelectedId(null);
    setPositions({});
    setCollapsedIds(new Set());
  }

  async function loadGraph(nextContinuation: string | null = null) {
    const querySequence = ++graphQuerySequenceRef.current;
    setLoading(true);
    setError(null);
    onStatus?.("正在查询关系图…");
    try {
      const next = await queryGraph(
        root,
        createGraphRequest(
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
      if (querySequence !== graphQuerySequenceRef.current) {
        return;
      }
      const merged = nextContinuation && response ? mergeGraphResponses(response, next) : next;
      const viewKey = `${direction}:${depth}:${search.trim()}:${confidence}:${relationFilter}:${rootIds.join(",")}:${pathTarget ?? ""}`;
      const viewLabel = [
        GRAPH_KINDS.find((item) => item.value === graphKind)?.label ?? graphKind,
        `方向=${DIRECTIONS.find((item) => item.value === direction)?.label ?? direction}`,
        `深度=${depth}`,
        `搜索=${search.trim() || "全部"}`,
        `置信度=${confidence === "all" ? "全部" : confidence}`,
        `关系=${relationFilter.trim() || "全部"}`,
        `根=${rootIds.length > 0 ? rootIds.join(",") : "自动"}`,
        `目标=${pathTarget ?? "无"}`
      ].join(" · ");
      setResponse(merged);
      setResponseDirection(direction);
      if (!nextContinuation) setCollapsedIds(new Set());
      setActiveView(viewLabel);
      setSelectedId((current) =>
        current && merged.nodes.some((node) => node.id === current) ? current : merged.nodes[0]?.id ?? null
      );
      let saved: SavedGraphLayout | null = null;
      try {
        saved = await loadGraphLayout(root, projectId, graphKind, viewKey);
      } catch {
        // Layout storage is best-effort; graph data remains usable when upgrading an old index.
      }
      if (querySequence !== graphQuerySequenceRef.current) {
        return;
      }
      const previousPositions = nextContinuation ? positionsRef.current : {};
      const retained = { ...saved?.positions, ...previousPositions };
      if (nextContinuation && merged.nodes.some(node => retained[node.id])) {
        merged.nodes.forEach((node, index) => {
          retained[node.id] ??= fallbackPosition(index, merged.nodes.length);
        });
      }
      layoutFor(merged.nodes, retained);
      onStatus?.(null);
    } catch (err) {
      if (querySequence !== graphQuerySequenceRef.current) {
        return;
      }
      const message = err instanceof Error ? err.message : String(err);
      setError(message);
      onStatus?.(null);
    } finally {
      if (querySequence === graphQuerySequenceRef.current) {
        setLoading(false);
      }
    }
  }

  useEffect(() => {
    if (
      (requestedGraphKind &&
        consumedGraphKindRef.current !== requestedGraphKind &&
        graphKind !== requestedGraphKind) ||
      (requestedGraphNodeId &&
        consumedGraphNodeRef.current !== requestedGraphNodeId &&
        !rootIds.includes(requestedGraphNodeId))
    ) {
      return;
    }
    void loadGraph();
    // The explicit controls below are the intended query boundaries.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [root, projectId, graphKind, rootIds, pathTarget, requestedGraphKind, requestedGraphNodeId, graphDataVersion]);

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
    return renderGraphSvgSnapshot(visibleNodes, visibleEdges, positions, pathNodes, centralNodes, highlightedEdges);
  }

  useEffect(() => {
    onGraphSnapshot?.({
      svg: visibleNodes.length > 0 ? graphSvgSnapshot() : "",
      view: activeView
    });
  }, [visibleNodes, visibleEdges, positions, pathNodes, centralNodes, activeView, onGraphSnapshot]);

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
    ? visibleEdges.filter((edge) => edge.source === selected.id || edge.target === selected.id)
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
          <button className="secondary" disabled={visibleNodes.length === 0} onClick={saveCurrentLayout}>
            保存布局
          </button>
          <button className="secondary" disabled={visibleNodes.length === 0} onClick={exportSvg}>
            导出 SVG
          </button>
          <button className="secondary" disabled={visibleNodes.length === 0} onClick={exportPng}>
            导出 PNG
          </button>
        </div>
      </div>

      <div className="graph-controls">
        <label>
          <span>图类型</span>
          <select value={graphKind} onChange={(event) => changeGraphKind(event.target.value as GraphKind)}>
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
            <option value="compile-db">编译数据库</option>
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

      {selected && ["function", "symbol", "entry"].includes(selected.kind) ? <div className="graph-task-actions" aria-label="代码调查快捷操作">
        <span>围绕 {selected.label}：</span>
        <button className="secondary" disabled={loading} onClick={() => investigate(selected, "impact")}>改这里会影响哪里</button>
        <button className="secondary" disabled={loading} onClick={() => investigate(selected, "callers")}>谁调用它</button>
        <button className="secondary" disabled={loading} onClick={() => investigate(selected, "callees")}>它调用谁</button>
        <button className="secondary" disabled={loading} onClick={() => investigate(selected, "nearby")}>只看一层邻域</button>
      </div> : null}
      {impactTarget ? <ImpactPanel key={`${root}:${projectId}:${impactTarget.id}`} root={root} projectId={projectId} target={impactTarget} generation={graphDataVersion}
        onClose={() => setImpactTarget(null)} onLocate={node => investigate(node, "nearby")} onSource={showSource} /> : null}
      {sourceLocation ? <SourcePreviewPanel key={`${root}:${projectId}:${sourceLocation.file}:${sourceLocation.line}`} root={root} projectId={projectId} {...sourceLocation} generation={graphDataVersion} onClose={() => setSourceLocation(null)} /> : null}

      {error ? <div className="banner error">关系图查询失败：{error}</div> : null}
      {response ? (
        <div className="graph-status-row">
          <span>{nodes.length} / {response.statistics.original_node_count} 节点</span>
          <span>{edges.length} / {response.statistics.original_edge_count} 条关系</span>
          <span>置信度：{response.confidence.overall}</span>
          <span>循环：{response.statistics.cycle_count}</span>
          <span>分组：{response.groups.length}</span>
          <span>关键节点：{response.statistics.central_node_ids.length}</span>
          {hiddenNodeIds.size > 0 ? <span>已收起：{hiddenNodeIds.size}</span> : null}
          {response.truncated ? (
            <span className="graph-truncated">
              已截断：按页加载（{response.truncation_reason || "node_limit"}）
            </span>
          ) : null}
        </div>
      ) : null}

      <div className="graph-layout">
        <div className="graph-canvas-wrap">
          {visibleNodes.length === 0 ? (
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
              {visibleEdges.map((edge) => {
                const source = positions[edge.source];
                const target = positions[edge.target];
                if (!source || !target) return null;
                const highlighted = highlightedEdges.has(edge.id);
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
              {visibleNodes.map((node) => {
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
            <button className="graph-more" disabled={!response.continuation || loading} onClick={() => response.continuation && void loadGraph(response.continuation)}>
              加载下一页（{response.continuation || "无 continuation"}）
            </button>
          ) : null}
          {visibleNodes.length > 0 ? (
            <svg className="graph-minimap" viewBox={`0 0 ${CANVAS_WIDTH} ${CANVAS_HEIGHT}`} role="img" aria-label="关系图小地图">
              <rect width={CANVAS_WIDTH} height={CANVAS_HEIGHT} fill="#0d141b" />
              {visibleEdges.map((edge) => {
                const source = positions[edge.source];
                const target = positions[edge.target];
                return source && target ? <line key={edge.id} x1={source.x} y1={source.y} x2={target.x} y2={target.y} stroke="#7890a8" strokeWidth="3" /> : null;
              })}
              {visibleNodes.map((node) => {
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
                  <button className="secondary" onClick={() => onAskAi?.(`请解释 ${selected.qualified_name} 的职责、调用关系和潜在影响面。`, selected.id)}>
                    问 AI
                  </button>
                </div>
              ) : null}
              <div className="graph-node-actions">
                <button
                  className="secondary"
                  onClick={() => setCollapsedIds((current) => {
                    const next = new Set(current);
                    if (next.has(selected.id)) next.delete(selected.id);
                    else next.add(selected.id);
                    return next;
                  })}
                >
                  {collapsedIds.has(selected.id) ? "展开下游" : "收起下游"}
                </button>
                <button
                  className="secondary"
                  onClick={() => setRootIds([selected.id])}
                >
                  设为查询根
                </button>
                {rootIds.length > 0 ? (
                  <button className="secondary" onClick={() => setRootIds([])}>
                    清除查询根
                  </button>
                ) : null}
                <button
                  className="secondary"
                  onClick={() => setPathTarget(selected.id)}
                >
                  设为路径目标
                </button>
                {pathTarget ? (
                  <button className="secondary" onClick={() => setPathTarget(null)}>
                    清除路径目标
                  </button>
                ) : null}
              </div>
              {rootIds.length > 0 || pathTarget ? (
                <p className="muted graph-query-hint">
                  根：{rootIds.join(", ") || "自动"} · 目标：{pathTarget || "未设置"}（根和目标会自动刷新；搜索/筛选请点“应用筛选”）
                </p>
              ) : null}
              <dl className="graph-details">
                <dt>稳定 ID</dt><dd><code>{selected.id}</code></dd>
                <dt>限定名</dt><dd>{selected.qualified_name || "—"}</dd>
                <dt>位置</dt><dd>{selected.file ? <button className="source-link" onClick={() => showSource(selected.file, selected.line)}>{selected.file}:{selected.line}</button> : "—"}</dd>
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
                        <button className="source-link" key={item.id} onClick={() => showSource(item.file, item.line)}>{item.file}:{item.line}</button>
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
