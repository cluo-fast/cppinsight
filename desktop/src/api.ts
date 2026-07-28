import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type {
  AskResponse,
  IndexProgressEvent,
  InspectSymbolResponse,
  LoadedWorkspace,
  OpenAiSettings,
  PersistedChat,
  ProjectOverview,
  ScanResult,
  WorkspaceSnapshot
} from "./types";

export function formatIndexProgress(event: IndexProgressEvent): string {
  if (event.stage === "indexing") {
    return event.message || "正在写入索引…";
  }
  if (event.stage === "done") {
    return event.message || "索引完成";
  }
  if (event.files_discovered != null) {
    const analyzed = event.files_analyzed ?? 0;
    const current = event.current_file ? `（${event.current_file}）` : "";
    return `正在扫描… 已发现 ${event.files_discovered} 个文件，分析 ${analyzed}${current}`;
  }
  return event.message || "正在扫描代码并建立索引…";
}

export async function chooseProjectDirectory(): Promise<string | null> {
  const selected = await open({
    directory: true,
    multiple: false,
    title: "Open C/C++ Project"
  });

  return typeof selected === "string" ? selected : null;
}

export function scanProject(path: string): Promise<ScanResult> {
  return invoke<ScanResult>("scan_project", { path });
}

export function indexProject(path: string): Promise<ProjectOverview> {
  return invoke<ProjectOverview>("index_project", { path });
}

/** Load existing project-local index; `null` if not indexed yet. */
export function loadIndexedProject(path: string): Promise<ProjectOverview | null> {
  return invoke<ProjectOverview | null>("load_indexed_project", { path });
}

export function indexScanJson(scanJson: string): Promise<ProjectOverview> {
  return invoke<ProjectOverview>("index_scan_json", { scanJson });
}

export function getProjectOverview(root: string, projectId: number): Promise<ProjectOverview> {
  return invoke<ProjectOverview>("get_project_overview", { root, projectId });
}

export function setOpenAiApiKey(apiKey: string): Promise<void> {
  return invoke<void>("set_openai_api_key", { apiKey });
}

export function getOpenAiSettings(): Promise<OpenAiSettings> {
  return invoke<OpenAiSettings>("get_openai_settings");
}

export function setOpenAiSettings(settings: OpenAiSettings): Promise<void> {
  return invoke<void>("set_openai_settings", { settings });
}

export function askProject(
  root: string,
  projectId: number,
  question: string
): Promise<AskResponse> {
  return invoke<AskResponse>("ask_project", { root, projectId, question });
}

export function exportClientSummary(root: string, projectId: number): Promise<string> {
  return invoke<string>("export_client_summary", { root, projectId });
}

export function inspectSymbol(
  root: string,
  projectId: number,
  query: string,
  depth?: number
): Promise<InspectSymbolResponse> {
  return invoke<InspectSymbolResponse>("inspect_symbol", {
    root,
    projectId,
    query,
    depth: depth ?? null
  });
}

export function loadWorkspace(): Promise<LoadedWorkspace> {
  return invoke<LoadedWorkspace>("load_workspace");
}

export function saveWorkspace(snapshot: WorkspaceSnapshot): Promise<void> {
  return invoke<void>("save_workspace", { snapshot });
}

export function loadProjectChats(rootKey: string): Promise<PersistedChat[]> {
  return invoke<PersistedChat[]>("load_project_chats", { rootKey });
}
