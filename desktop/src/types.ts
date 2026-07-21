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
}

export interface SymbolInfo {
  file: string;
  line: number;
  kind: string;
  name: string;
  signature: string;
  scope: string;
  is_definition: boolean;
}

export interface CallInfo {
  file: string;
  line: number;
  name: string;
}

export interface FileInfo {
  path: string;
  extension: string;
  category: string;
  size_bytes: number;
  lines: LineStats;
  includes: IncludeInfo[];
  symbols: SymbolInfo[];
  calls: CallInfo[];
}

export interface ScanResult {
  schema_version: number;
  root: string;
  file_count: number;
  skipped_entries: number;
  stats: ScanStats;
  extensions: Record<string, number>;
  top_level_layout: Record<string, number>;
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
  stats: ScanStats;
  extensions: Record<string, number>;
  top_level_layout: Record<string, number>;
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

export interface ChatMessage {
  id: string;
  role: "user" | "assistant";
  text: string;
  citations?: Citation[];
}
