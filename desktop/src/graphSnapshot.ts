import type { GraphEdge, GraphNode, GraphPosition } from "./types";

export const GRAPH_CANVAS_WIDTH = 1000;
export const GRAPH_CANVAS_HEIGHT = 620;

export function nodeColor(node: GraphNode): string {
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

export function shortLabel(node: GraphNode): string {
  const value = node.label || node.qualified_name || node.id;
  return value.length > 24 ? `${value.slice(0, 23)}…` : value;
}

function escapeXml(value: string): string {
  return value.replace(/[<>&'\"]/g, (character) => ({
    "<": "&lt;",
    ">": "&gt;",
    "&": "&amp;",
    "'": "&apos;",
    "\"": "&quot;"
  }[character] ?? character));
}

function safeCoordinate(value: number, fallback: number): number {
  return Number.isFinite(value) ? value : fallback;
}

export function graphSvgSnapshot(
  nodes: GraphNode[],
  edges: GraphEdge[],
  positions: Record<string, GraphPosition>,
  pathNodes: ReadonlySet<string>,
  centralNodes: ReadonlySet<string>
): string {
  const lines = edges.map((edge) => {
    const source = positions[edge.source];
    const target = positions[edge.target];
    if (!source || !target) return "";
    const sourceX = safeCoordinate(source.x, 80);
    const sourceY = safeCoordinate(source.y, 80);
    const targetX = safeCoordinate(target.x, 80);
    const targetY = safeCoordinate(target.y, 80);
    const highlighted = pathNodes.has(edge.source) && pathNodes.has(edge.target);
    return `<line x1="${sourceX}" y1="${sourceY}" x2="${targetX}" y2="${targetY}" stroke="${highlighted ? "#ffd166" : "#7890a8"}" stroke-width="${Math.min(6, 1 + edge.weight / 2)}" marker-end="url(#arrow)"/>`;
  }).join("");
  const circles = nodes.map((node) => {
    const position = positions[node.id] ?? { x: 80, y: 80 };
    const x = safeCoordinate(position.x, 80);
    const y = safeCoordinate(position.y, 80);
    const radius = centralNodes.has(node.id) ? 17 : 13;
    return `<circle cx="${x}" cy="${y}" r="${radius}" fill="${nodeColor(node)}"/><text x="${x}" y="${y + 30}" text-anchor="middle" fill="#e8eef5" font-size="11">${escapeXml(shortLabel(node))}</text>`;
  }).join("");
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${GRAPH_CANVAS_WIDTH}" height="${GRAPH_CANVAS_HEIGHT}" viewBox="0 0 ${GRAPH_CANVAS_WIDTH} ${GRAPH_CANVAS_HEIGHT}"><defs><marker id="arrow" markerWidth="8" markerHeight="8" refX="7" refY="4" orient="auto"><path d="M0,0 L8,4 L0,8 z" fill="#7890a8"/></marker></defs><rect width="100%" height="100%" fill="#0d141b"/>${lines}${circles}</svg>`;
}
