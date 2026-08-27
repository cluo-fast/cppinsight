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
  file: string;
  line: number;
  kind: string;
  signature: string;
  is_definition: boolean;
  confidence: string;
}

export interface ImpactNeighbor {
  name: string;
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
