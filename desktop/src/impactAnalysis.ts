import type { GraphDirection, GraphEdge, GraphNode, GraphQueryResponse } from "./types";

export interface ImpactItem {
  node: GraphNode;
  distance: number;
  path: GraphNode[];
  edges: GraphEdge[];
  uncertain: boolean;
}

/** Walk caller -> callee edges in reverse; never infer impact from downstream nodes. */
export function summarizeImpact(graph: GraphQueryResponse, rootId: string) {
  const nodes = new Map(graph.nodes.map(node => [node.id, node]));
  const incoming = new Map<string, GraphEdge[]>();
  for (const edge of [...graph.edges].sort((a, b) => a.id.localeCompare(b.id))) {
    if (edge.kind !== "calls" || !nodes.has(edge.source) || !nodes.has(edge.target)) continue;
    const list = incoming.get(edge.target) ?? [];
    list.push(edge);
    incoming.set(edge.target, list);
  }
  const root = nodes.get(rootId);
  const paths = new Map<string, GraphEdge[]>();
  if (root) paths.set(rootId, []);
  const queue = root ? [rootId] : [];
  const directIds = new Set((incoming.get(rootId) ?? []).map(edge => edge.source));
  for (let cursor = 0; cursor < queue.length; cursor++) {
    const id = queue[cursor];
    for (const edge of incoming.get(id) ?? []) {
      if (paths.has(edge.source)) continue;
      paths.set(edge.source, [edge, ...paths.get(id)!]);
      queue.push(edge.source);
    }
  }
  const items: ImpactItem[] = queue.filter(id => id !== rootId).map(id => {
    const edges = paths.get(id)!;
    const path = [nodes.get(id)!, ...edges.map(edge => nodes.get(edge.target)!)];
    return {
      node: nodes.get(id)!, distance: edges.length, path, edges,
      uncertain: edges.some(edge => edge.ambiguous || !["resolved", "precise"].includes(edge.confidence))
        || path.some(node => node.ambiguous || node.kind === "unresolved"
          || (node.confidence && !["resolved", "precise"].includes(node.confidence)))
    };
  }).sort((a, b) => a.distance - b.distance || a.node.id.localeCompare(b.node.id));
  return {
    root,
    items,
    direct: items.filter(item => directIds.has(item.node.id)),
    indirect: items.filter(item => !directIds.has(item.node.id)),
    entries: items.filter(item => item.node.kind === "entry"),
    files: [...new Set(items.map(item => item.node.file).filter(Boolean))].sort(),
    modules: [...new Set(items.map(item => item.node.module_id).filter(Boolean))].sort(),
    partial: graph.truncated || graph.statistics.original_node_count > graph.nodes.length
  };
}

export function pathEdgeIds(edges: GraphEdge[], path: string[], direction: GraphDirection = "both"): Set<string> {
  const pairs = new Set(path.slice(1).map((id, index) => JSON.stringify([path[index], id])));
  return new Set(edges.filter(edge => (direction !== "incoming" && pairs.has(JSON.stringify([edge.source, edge.target])))
    || (direction !== "outgoing" && pairs.has(JSON.stringify([edge.target, edge.source])))).map(edge => edge.id));
}
