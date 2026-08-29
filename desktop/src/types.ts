export interface LineStats {
  total: number;
  code: number;
  comment: number;
  blank: number;
}

export interface ScanStats {
  source_files: number;
  header_files: number;
  inline_files: number;
  total_bytes: number;
  lines: LineStats;
}

export interface IncludeInfo {
  file: string;
  line: number;
  target: string;
  kind: string;
  resolved?: boolean;
  resolved_path?: string;
}

export interface SymbolInfo {
  file: string;
  line: number;
  kind: string;
  name: string;
  signature: string;
  scope: string;
  is_definition: boolean;
  confidence?: string;
}

export interface CallInfo {
  file: string;
  line: number;
  name: string;
  confidence?: string;
}

export interface FileInfo {
  path: string;
  extension: string;
  category: string;
  size_bytes: number;
  mtime_unix?: number;
  from_compile_commands?: boolean;
  reused_from_cache?: boolean;
  lines: LineStats;
  includes: IncludeInfo[];
  symbols: SymbolInfo[];
  calls: CallInfo[];
}

export interface ScanResult {
  schema_version: number;
  root: string;
  mode?: string;
  confidence?: string;
  scan_duration_ms?: number;
  file_count: number;
  skipped_entries: number;
  stats: ScanStats;
  extensions: Record<string, number>;
  top_level_layout: Record<string, number>;
  second_level_layout?: Record<string, number>;
  diagnostics: string[];
  includes: IncludeInfo[];
  symbols: SymbolInfo[];
  calls: CallInfo[];
  files: FileInfo[];
}

export interface HotFile {
  path: string;
  total_lines: number;
  code_lines: number;
  symbol_count: number;
}

export interface ProjectOverview {
  project_id: number;
  root: string;
  indexed_at: string;
  file_count: number;
  symbol_count: number;
  include_count: number;
  mode?: string;
  confidence?: string;
  stats: ScanStats;
  extensions: Record<string, number>;
  top_level_layout: Record<string, number>;
  second_level_layout?: Record<string, number>;
  hotspots: HotFile[];
  symbols: SymbolInfo[];
  diagnostics: string[];
}

export interface Citation {
  file: string;
  line: number;
  title: string;
}

export interface AskResponse {
  answer: string;
  citations: Citation[];
}

export interface SymbolHit {
  name: string;
  stable_id?: string;
  file: string;
  line: number;
  kind: string;
  signature: string;
  is_definition: boolean;
  confidence: string;
}

export interface ImpactNeighbor {
  name: string;
  stable_id?: string;
  file: string;
  line: number;
  via_file: string;
  via_line: number;
}

export interface EntryPath {
  entry: string;
  hops: SymbolHit[];
}

export interface ImpactReport {
  symbol: string;
  definitions: SymbolHit[];
  callers: ImpactNeighbor[];
  callees: ImpactNeighbor[];
  entry_paths: EntryPath[];
  confidence: string;
}

export interface InspectSymbolResponse {
  query: string;
  message: string;
  candidates: SymbolHit[];
  report: ImpactReport | null;
}

export type ComposerMode = "ask" | "symbol";

export interface ChatMessage {
  id: string;
  role: "user" | "assistant";
  text: string;
  citations?: Citation[];
  graph_node_id?: string;
  kind?: "text" | "symbol-inspect";
  inspect?: InspectSymbolResponse;
}

export interface IndexProgressEvent {
  stage: "scanning" | "indexing" | "done" | string;
  files_discovered?: number;
  files_analyzed?: number;
  files_reused?: number;
  current_file?: string;
  message?: string;
}

export interface PublicOpenAiSettings {
    base_url: string;
    model: string;
    api_key_configured: boolean;
}

export interface UpdateOpenAiSettings {
  base_url: string;
  model: string;
  api_key: string | null;
  clear_api_key: boolean;
}

export interface DiagnosticProject {
  project_id: string;
  index_status: string;
  file_count: number;
  symbol_count: number;
  include_count: number;
  conversation_count: number;
  mode: string;
  confidence: string;
  diagnostic_categories: string[];
}

export interface DiagnosticReport {
  schema_version: number;
  generated_at: string;
  app_version: string;
  platform: { os: string; arch: string };
  redaction_notice: string;
  projects: DiagnosticProject[];
}

export interface PersistedMessage {
  id: string;
  role: string;
  content_json: string;
  sort_order: number;
}

export interface PersistedChat {
  id: string;
  root_key: string;
  title: string;
  composer_mode: string;
  sort_order: number;
  active: boolean;
  messages: PersistedMessage[];
}

export interface WorkspaceSnapshot {
  active_root_key: string | null;
  open_projects: { root_key: string; root_path: string; sort_order: number }[];
  chats: PersistedChat[];
}

export interface LoadedProject {
  root_key: string;
  root_path: string;
  index_missing: boolean;
  overview: ProjectOverview | null;
  chats: PersistedChat[];
  active_chat_id: string | null;
}

export interface LoadedWorkspace {
  active_root_key: string | null;
  projects: LoadedProject[];
}

export type GraphKind =
  | "module_dependencies"
  | "function_calls"
  | "class_relations"
  | "file_includes"
  | "entry_paths";

export type GraphDirection = "incoming" | "outgoing" | "both";
export type GraphConfidence =
  | "resolved"
  | "precise"
  | "hybrid"
  | "heuristic"
  | "ambiguous"
  | "unresolved";
export type GraphNodeKind =
  | "module"
  | "file"
  | "function"
  | "class"
  | "struct"
  | "enum"
  | "symbol"
  | "entry"
  | "unresolved"
  | "group";

export interface GraphFilters {
  module_ids: string[];
  file_paths: string[];
  symbol_kinds: string[];
  relation_kinds: string[];
  confidence: GraphConfidence[];
  search: string | null;
}

export interface GraphQueryRequest {
  protocol_version: number;
  project_id: number;
  graph_kind: GraphKind;
  root_node_ids: string[];
  direction: GraphDirection;
  depth: number;
  filters: GraphFilters;
  max_nodes: number;
  path_target: string | null;
  continuation: string | null;
}

export interface GraphNode {
  id: string;
  kind: GraphNodeKind;
  label: string;
  qualified_name: string;
  file: string;
  line: number;
  module_id: string;
  confidence: GraphConfidence;
  ambiguous: boolean;
  metadata: Record<string, string>;
}

export interface GraphEdge {
  id: string;
  source: string;
  target: string;
  kind: string;
  weight: number;
  confidence: GraphConfidence;
  ambiguous: boolean;
  evidence_ids: string[];
}

export interface GraphGroup {
  id: string;
  label: string;
  kind: string;
  node_ids: string[];
}

export interface GraphEvidence {
  id: string;
  edge_id: string;
  file: string;
  line: number;
  detail: string;
  confidence: GraphConfidence;
}

export interface GraphConfidenceSummary {
  overall: GraphConfidence;
  resolved: number;
  precise: number;
  hybrid: number;
  heuristic: number;
  ambiguous: number;
  unresolved: number;
}

export interface GraphStatistics {
  original_node_count: number;
  returned_node_count: number;
  original_edge_count: number;
  returned_edge_count: number;
  cycle_count: number;
  unresolved_edge_count: number;
  ambiguous_edge_count: number;
  central_node_ids: string[];
  shortest_path_node_ids: string[];
}

export interface GraphQueryResponse {
  protocol_version: number;
  project_id: number;
  graph_kind: GraphKind;
  nodes: GraphNode[];
  edges: GraphEdge[];
  groups: GraphGroup[];
  evidence: GraphEvidence[];
  confidence: GraphConfidenceSummary;
  statistics: GraphStatistics;
  truncated: boolean;
  truncation_reason: string | null;
  continuation: string | null;
}

export interface GraphPosition {
  x: number;
  y: number;
}

export interface SavedGraphLayout {
  protocol_version: number;
  project_id: number;
  graph_kind: GraphKind;
  view_key: string;
  positions: Record<string, GraphPosition>;
  updated_at: string;
}
