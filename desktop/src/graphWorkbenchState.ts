import type {
  GraphConfidence,
  GraphDirection,
  GraphQueryRequest,
  GraphQueryResponse
} from "./types";

export function createGraphRequest(
  projectId: number,
  graphKind: GraphQueryRequest["graph_kind"],
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
      relation_kinds: relationFilter
        .trim()
        .split(",")
        .map((item) => item.trim())
        .filter(Boolean),
      confidence: confidence === "all" ? [] : [confidence],
      search: search.trim() || null
    },
    max_nodes: 500,
    path_target: pathTarget,
    continuation
  };
}

export function mergeGraphResponses(
  previous: GraphQueryResponse,
  next: GraphQueryResponse
): GraphQueryResponse {
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
