import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type { AskResponse, ProjectOverview, ScanResult } from "./types";

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

export function getProjectOverview(projectId: number): Promise<ProjectOverview> {
  return invoke<ProjectOverview>("get_project_overview", { projectId });
}

export function setOpenAiApiKey(apiKey: string): Promise<void> {
  return invoke<void>("set_openai_api_key", { apiKey });
}

export function askProject(projectId: number, question: string): Promise<AskResponse> {
  return invoke<AskResponse>("ask_project", { projectId, question });
}
