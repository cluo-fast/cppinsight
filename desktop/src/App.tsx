import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getVersion } from "@tauri-apps/api/app";
import React, { useEffect, useMemo, useRef, useState } from "react";
import {
  askProject,
  chooseProjectDirectory,
  exportDiagnostics,
  exportClientSummary,
  formatIndexProgress,
  getOpenAiSettings,
  indexProject,
  inspectSymbol,
  loadIndexedProject,
  loadProjectChats,
  loadWorkspace,
  previewDiagnostics,
  prepareSampleProject,
  saveWorkspace,
  setOpenAiSettings
} from "./api";
import GraphWorkbench from "./GraphWorkbench";
import type {
  AskResponse,
  ChatMessage,
  ComposerMode,
  GraphKind,
  IndexProgressEvent,
  InspectSymbolResponse,
  PersistedChat,
  PersistedMessage,
  ProjectOverview,
  SymbolInfo,
  WorkspaceSnapshot
} from "./types";
import "./styles.css";

const DEFAULT_AI_SETTINGS: OpenAiSettings = {
  base_url: "https://api.openai.com/v1",
  api_key: "",
  model: "gpt-4o-mini",
  api_key_configured: false,
  clear_api_key: false
};

type OpenAiSettings = {
  base_url: string;
  api_key: string;
  model: string;
  api_key_configured: boolean;
  clear_api_key: boolean;
};

type ChatThread = {
  id: string;
  title: string;
  messages: ChatMessage[];
  question: string;
  composerMode: ComposerMode;
  running: boolean;
};

type ProjectSession = {
  id: string;
  overview: ProjectOverview;
  summary: string;
  indexing: boolean;
  indexMissing?: boolean;
  chats: ChatThread[];
  activeChatId: string;
};

function explainImpactPrompt(inspect: InspectSymbolResponse): string {
  const report = inspect.report;
  if (!report) {
    return `请根据索引解释符号候选：${inspect.candidates
      .slice(0, 8)
      .map((item) => `${item.name} @ ${item.file}:${item.line}`)
      .join("；")}`;
  }
  const callers = report.callers
    .slice(0, 8)
    .map((item) => `${item.name} (${item.file}:${item.line})`)
    .join("；");
  const callees = report.callees
    .slice(0, 8)
    .map((item) => `${item.name} (${item.file}:${item.line})`)
    .join("；");
  const paths = report.entry_paths
    .slice(0, 3)
    .map((path) => path.hops.map((hop) => hop.name).join(" → "))
    .join(" | ");
  return [
    `请解释符号 ${report.symbol} 在本项目中的角色与影响面（基于本地启发式索引，可能不完整）。`,
    `上游调用方：${callers || "无"}`,
    `下游被调：${callees || "无"}`,
    `入口路径：${paths || "未发现"}`
  ].join("\n");
}

function downloadText(text: string, filename: string, type: string) {
  const url = URL.createObjectURL(new Blob([text], { type }));
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = filename;
  anchor.click();
  URL.revokeObjectURL(url);
}

function summaryAsHtml(markdown: string, title: string, graphSvg = "", graphView = ""): string {
  const escape = (value: string) => value.replace(/[&<>\"]/g, (character) => ({
    "&": "&amp;", "<": "&lt;", ">": "&gt;", "\"": "&quot;"
  }[character] ?? character));
  const body = markdown.split(/\r?\n/).map((line) => {
    if (line.startsWith("### ")) return `<h3>${escape(line.slice(4))}</h3>`;
    if (line.startsWith("## ")) return `<h2>${escape(line.slice(3))}</h2>`;
    if (line.startsWith("# ")) return `<h1>${escape(line.slice(2))}</h1>`;
    if (line.startsWith("- ")) return `<li>${escape(line.slice(2))}</li>`;
    return line.trim() ? `<p>${escape(line)}</p>` : "";
  }).join("\n").replace(/(<li>.*<\/li>\n?)+/g, (items) => `<ul>${items}</ul>`);
  const graphSection = graphSvg
    ? `<section><h2>关系图快照</h2>${graphView ? `<p><strong>视图：</strong>${escape(graphView)}</p>` : ""}${graphSvg}</section>`
    : "";
  return `<!doctype html><html lang="zh-CN"><head><meta charset="utf-8"><title>${escape(title)}</title><style>body{font:16px system-ui;max-width:960px;margin:40px auto;line-height:1.6;color:#1f2937}code{background:#f3f4f6;padding:2px 4px}h1,h2,h3{margin-top:1.5em}section svg{max-width:100%;height:auto;border:1px solid #d1d5db}</style></head><body>${body}${graphSection}</body></html>`;
}

function createId(): string {
  return `${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

function normalizeRootKey(path: string): string {
  return path.replace(/\\/g, "/").replace(/\/+$/, "").toLowerCase();
}

function splitDisplayPath(path: string): { name: string; dir: string; full: string } {
  const full = path.replace(/\\/g, "/");
  const parts = full.split("/").filter(Boolean);
  const name = parts[parts.length - 1] || full;
  const dir = parts.length > 1 ? parts.slice(0, -1).join("/") : ".";
  return { name, dir, full };
}

function titleFromText(text: string): string {
  const cleaned = text.replace(/\s+/g, " ").trim();
  if (!cleaned) {
    return "新对话";
  }
  return cleaned.length > 28 ? `${cleaned.slice(0, 28)}…` : cleaned;
}

function createChat(title = "新对话"): ChatThread {
  return {
    id: createId(),
    title,
    messages: [],
    question: "",
    composerMode: "ask",
    running: false
  };
}

function placeholderOverview(rootPath: string): ProjectOverview {
  return {
    project_id: -1,
    root: rootPath,
    indexed_at: "",
    file_count: 0,
    symbol_count: 0,
    include_count: 0,
    stats: {
      source_files: 0,
      header_files: 0,
      inline_files: 0,
      total_bytes: 0,
      lines: { total: 0, code: 0, comment: 0, blank: 0 }
    },
    extensions: {},
    top_level_layout: {},
    hotspots: [],
    symbols: [],
    diagnostics: ["本地索引不可用：未找到 .cppinsight/index.sqlite 或项目记录"]
  };
}

function messageToPersisted(message: ChatMessage, sortOrder: number): PersistedMessage {
  return {
    id: message.id,
    role: message.role,
    content_json: JSON.stringify(message),
    sort_order: sortOrder
  };
}

function messageFromPersisted(persisted: PersistedMessage): ChatMessage {
  try {
    return JSON.parse(persisted.content_json) as ChatMessage;
  } catch {
    return {
      id: persisted.id,
      role: persisted.role === "assistant" ? "assistant" : "user",
      text: persisted.content_json,
      kind: "text"
    };
  }
}

function chatsFromPersisted(persisted: PersistedChat[]): {
  chats: ChatThread[];
  activeChatId: string;
} {
  if (persisted.length === 0) {
    const first = createChat("对话 1");
    return { chats: [first], activeChatId: first.id };
  }
  const chats: ChatThread[] = persisted
    .slice()
    .sort((a, b) => a.sort_order - b.sort_order)
    .map((chat) => ({
      id: chat.id,
      title: chat.title,
      messages: chat.messages
        .slice()
        .sort((a, b) => a.sort_order - b.sort_order)
        .map(messageFromPersisted),
      question: "",
      composerMode: chat.composer_mode === "symbol" ? "symbol" : "ask",
      running: false
    }));
  const active =
    persisted.find((chat) => chat.active)?.id ?? chats[0]?.id ?? createChat("对话 1").id;
  return { chats, activeChatId: active };
}

const ENTRY_NAMES = new Set([
  "main",
  "wmain",
  "WinMain",
  "wWinMain",
  "DllMain",
  "_tmain",
  "_tWinMain"
]);

function entryPoints(symbols: SymbolInfo[]): SymbolInfo[] {
  return symbols.filter(
    (symbol) => symbol.kind === "function" && ENTRY_NAMES.has(symbol.name)
  );
}

const SUGGESTED_QUESTIONS = [
  "可能的程序入口在哪？",
  "这个项目的顶层目录分别大概负责什么？",
  "代码量最大的几个文件是做什么的？",
  "公共头文件有哪些，谁在用？"
];

function analysisModeLabel(mode: string): string {
  if (mode === "hybrid") {
    return "混合分析（已使用 compile_commands.json）";
  }
  if (mode === "heuristic") {
    return "启发式分析（未找到 compile_commands.json，关系可能不完整）";
  }
  return `分析模式：${mode || "未知"}`;
}

export default function App() {
  const [appVersion, setAppVersion] = useState("0.1.0");
  const [sessions, setSessions] = useState<ProjectSession[]>([]);
  const [activeRoot, setActiveRoot] = useState<string | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [aiSettings, setAiSettings] = useState<OpenAiSettings>(DEFAULT_AI_SETTINGS);
  const [showSettings, setShowSettings] = useState(false);
  const [settingsSaving, setSettingsSaving] = useState(false);
  const [diagnosticBusy, setDiagnosticBusy] = useState(false);
  const [diagnosticPreview, setDiagnosticPreview] = useState<string | null>(null);
  const [workspaceReady, setWorkspaceReady] = useState(false);
  const [showGraph, setShowGraph] = useState(false);
  const [requestedGraphKind, setRequestedGraphKind] = useState<GraphKind | null>(null);
  const [requestedGraphNodeId, setRequestedGraphNodeId] = useState<string | null>(null);
  const [graphRequestId, setGraphRequestId] = useState(0);
  const [graphDataVersion, setGraphDataVersion] = useState(0);
  const [graphSnapshot, setGraphSnapshot] = useState<{ svg: string; view: string } | null>(null);

  useEffect(() => {
    void getVersion()
      .then((version) => setAppVersion(version))
      .catch(() => {
        // Browser development mode has no Tauri app metadata; keep the package fallback.
      });
  }, []);

  useEffect(() => {
    setGraphSnapshot(null);
  }, [activeRoot]);

  const sessionsRef = useRef(sessions);
  sessionsRef.current = sessions;
  const activeRootRef = useRef(activeRoot);
  activeRootRef.current = activeRoot;
  const retainedChatsRef = useRef<Record<string, { chats: ChatThread[]; activeChatId: string }>>(
    {}
  );
  const summaryRequestSequenceRef = useRef(0);
  const summaryLoadingRef = useRef(new Map<string, number>());
  const indexingInFlightRef = useRef(false);
  const persistTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const persistSkipRef = useRef(true);

  const active = useMemo(
    () => sessions.find((session) => session.id === activeRoot) ?? null,
    [sessions, activeRoot]
  );

  function openReportGraph(kind: GraphKind) {
    setGraphRequestId((current) => current + 1);
    setRequestedGraphKind(kind);
    setRequestedGraphNodeId(null);
    setShowGraph(true);
  }

  function openSymbolGraph(stableId: string | undefined) {
    if (!stableId) return;
    setGraphRequestId((current) => current + 1);
    setRequestedGraphKind("function_calls");
    setRequestedGraphNodeId(stableId);
    setShowGraph(true);
  }

  const activeChat = useMemo(() => {
    if (!active) {
      return null;
    }
    return active.chats.find((chat) => chat.id === active.activeChatId) ?? active.chats[0] ?? null;
  }, [active]);

  useEffect(() => {
    const pendingSequence = active ? summaryLoadingRef.current.get(active.id) : undefined;
    if (active && pendingSequence != null && pendingSequence !== summaryRequestSequenceRef.current) {
      summaryLoadingRef.current.delete(active.id);
    }
    if (
      !active ||
      active.indexMissing ||
      active.indexing ||
      active.summary ||
      (pendingSequence != null && pendingSequence === summaryRequestSequenceRef.current)
    ) {
      return;
    }
    const sessionId = active.id;
    const requestSequence = ++summaryRequestSequenceRef.current;
    summaryLoadingRef.current.set(sessionId, requestSequence);
    void exportClientSummary(active.overview.root, active.overview.project_id)
      .then((text) => {
        if (requestSequence === summaryRequestSequenceRef.current) {
          patchProject(sessionId, { summary: text });
        }
      })
      .catch(() => {
        // The explicit export action remains available when report generation fails.
      })
      .finally(() => {
        if (summaryLoadingRef.current.get(sessionId) === requestSequence) {
          summaryLoadingRef.current.delete(sessionId);
        }
      });
  }, [
    active?.id,
    active?.indexMissing,
    active?.indexing,
    active?.overview.project_id,
    active?.overview.root,
    active?.summary
  ]);

  const overview = active?.overview ?? null;
  const anyIndexing = sessions.some((session) => session.indexing);

  function buildSnapshot(
    nextSessions: ProjectSession[],
    nextActive: string | null,
    retained: Record<string, { chats: ChatThread[]; activeChatId: string }>
  ): WorkspaceSnapshot {
    const open_projects = nextSessions.map((session, index) => ({
      root_key: session.id,
      root_path: session.overview.root,
      sort_order: index
    }));

    const chatRoots = new Map<string, { chats: ChatThread[]; activeChatId: string }>();
    for (const session of nextSessions) {
      chatRoots.set(session.id, { chats: session.chats, activeChatId: session.activeChatId });
    }
    for (const [rootKey, value] of Object.entries(retained)) {
      if (!chatRoots.has(rootKey)) {
        chatRoots.set(rootKey, value);
      }
    }

    const chats: PersistedChat[] = [];
    for (const [rootKey, value] of chatRoots) {
      value.chats.forEach((chat, index) => {
        chats.push({
          id: chat.id,
          root_key: rootKey,
          title: chat.title,
          composer_mode: chat.composerMode,
          sort_order: index,
          active: chat.id === value.activeChatId,
          messages: chat.messages.map((message, messageIndex) =>
            messageToPersisted(message, messageIndex)
          )
        });
      });
    }

    return {
      active_root_key: nextActive,
      open_projects,
      chats
    };
  }

  function schedulePersist() {
    if (persistSkipRef.current) {
      return;
    }
    if (persistTimerRef.current) {
      clearTimeout(persistTimerRef.current);
    }
    persistTimerRef.current = setTimeout(() => {
      const snapshot = buildSnapshot(
        sessionsRef.current,
        activeRootRef.current,
        retainedChatsRef.current
      );
      void saveWorkspace(snapshot).catch((err) => {
        console.error("save_workspace failed", err);
      });
    }, 300);
  }

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const loaded = await loadWorkspace();
        if (cancelled) {
          return;
        }
        const nextSessions: ProjectSession[] = loaded.projects.map((project) => {
          const { chats, activeChatId } = chatsFromPersisted(project.chats);
          const overview = project.overview ?? placeholderOverview(project.root_path);
          return {
            id: project.root_key || normalizeRootKey(project.root_path),
            overview,
            summary: "",
            indexing: false,
            indexMissing: project.index_missing || !project.overview,
            chats,
            activeChatId: project.active_chat_id ?? activeChatId
          };
        });
        setSessions(nextSessions);
        const active =
          loaded.active_root_key &&
          nextSessions.some((session) => session.id === loaded.active_root_key)
            ? loaded.active_root_key
            : nextSessions[0]?.id ?? null;
        setActiveRoot(active);
        if (nextSessions.length > 0) {
          setStatus(`已恢复 ${nextSessions.length} 个项目（来自上次会话）`);
        }
      } catch (err) {
        if (!cancelled) {
          setError(err instanceof Error ? err.message : String(err));
        }
      } finally {
        if (!cancelled) {
          persistSkipRef.current = false;
          setWorkspaceReady(true);
        }
      }
    })();
    return () => {
      cancelled = true;
      if (persistTimerRef.current) {
        clearTimeout(persistTimerRef.current);
      }
    };
  }, []);

  useEffect(() => {
    if (!workspaceReady) {
      return;
    }
    schedulePersist();
  }, [sessions, activeRoot, workspaceReady]);

  const modules = useMemo(() => {
    if (!overview) {
      return [];
    }
    const layout = overview.top_level_layout || overview.second_level_layout || {};
    return Object.entries(layout)
      .filter(([name]) => name !== ".cppinsight")
      .sort((left, right) => right[1] - left[1] || left[0].localeCompare(right[0]))
      .slice(0, 12);
  }, [overview]);

  const entries = useMemo(
    () => (overview ? entryPoints(overview.symbols) : []),
    [overview]
  );

  function patchProject(projectId: string, patch: Partial<Omit<ProjectSession, "id" | "chats">>) {
    setSessions((current) =>
      current.map((session) => (session.id === projectId ? { ...session, ...patch } : session))
    );
  }

  function patchChat(
    projectId: string,
    chatId: string,
    patch: Partial<Omit<ChatThread, "id">> | ((chat: ChatThread) => ChatThread)
  ) {
    setSessions((current) =>
      current.map((session) => {
        if (session.id !== projectId) {
          return session;
        }
        return {
          ...session,
          chats: session.chats.map((chat) => {
            if (chat.id !== chatId) {
              return chat;
            }
            return typeof patch === "function" ? patch(chat) : { ...chat, ...patch };
          })
        };
      })
    );
  }

  function updateActiveChat(patch: Partial<Omit<ChatThread, "id">>) {
    if (!active || !activeChat) {
      return;
    }
    patchChat(active.id, activeChat.id, patch);
  }

  async function resolveChatsForRoot(rootKey: string): Promise<{
    chats: ChatThread[];
    activeChatId: string;
  }> {
    const retained = retainedChatsRef.current[rootKey];
    if (retained && retained.chats.length > 0) {
      delete retainedChatsRef.current[rootKey];
      return retained;
    }
    try {
      const persisted = await loadProjectChats(rootKey);
      if (persisted.length > 0) {
        return chatsFromPersisted(persisted);
      }
    } catch (err) {
      console.error("load_project_chats failed", err);
    }
    const first = createChat("对话 1");
    return { chats: [first], activeChatId: first.id };
  }

  async function upsertSessionOverview(
    next: ProjectOverview,
    provisionalId?: string,
    options?: { indexMissing?: boolean }
  ) {
    summaryRequestSequenceRef.current += 1;
    const id = normalizeRootKey(next.root);
    summaryLoadingRef.current.delete(id);
    const restored = await resolveChatsForRoot(id);
    setSessions((current) => {
      const existing = current.find(
        (session) =>
          session.id === id || (provisionalId != null && session.id === provisionalId)
      );
      if (existing) {
        return current.map((session) =>
          session.id === existing.id
            ? {
                ...session,
                id,
                overview: next,
                summary: "",
                indexing: false,
                indexMissing: options?.indexMissing ?? false,
                chats: session.chats.length > 0 ? session.chats : restored.chats,
                activeChatId:
                  session.chats.find((chat) => chat.id === session.activeChatId)?.id ??
                  restored.activeChatId
              }
            : session
        );
      }
      return [
        ...current,
        {
          id,
          overview: next,
          summary: "",
          indexing: false,
          indexMissing: options?.indexMissing ?? false,
          chats: restored.chats,
          activeChatId: restored.activeChatId
        }
      ];
    });
    setActiveRoot(id);
  }

  async function runIndexAtPath(path: string, provisionalId: string) {
    if (indexingInFlightRef.current) {
      return;
    }
    indexingInFlightRef.current = true;
    const placeholderChat = createChat("对话 1");
    setSessions((current) =>
      current.some((session) => session.id === provisionalId)
        ? current.map((session) =>
            session.id === provisionalId ? { ...session, indexing: true } : session
          )
        : [
            ...current,
            {
              id: provisionalId,
              overview: placeholderOverview(path),
              summary: "",
              indexing: true,
              indexMissing: true,
              chats: [placeholderChat],
              activeChatId: placeholderChat.id
            }
          ]
    );
    setActiveRoot(provisionalId);
    setStatus("正在扫描代码并建立索引…");

    let unlisten: UnlistenFn | undefined;
    try {
      unlisten = await listen<IndexProgressEvent>("index-progress", (event) => {
        setStatus(formatIndexProgress(event.payload));
      });
      const next = await indexProject(path);
      await upsertSessionOverview(next, provisionalId, { indexMissing: false });
      setGraphDataVersion((current) => current + 1);
      setStatus(null);
    } finally {
      if (unlisten) {
        unlisten();
      }
      indexingInFlightRef.current = false;
    }
  }

  async function openProject() {
    setError(null);
    setStatus("正在选择项目目录…");
    let indexingId: string | null = null;
    try {
      const path = await chooseProjectDirectory();
      if (!path) {
        setStatus(null);
        return;
      }

      const provisionalId = normalizeRootKey(path);
      indexingId = provisionalId;

      setStatus("正在检查已有索引…");
      const existing = await loadIndexedProject(path);
      if (existing) {
        await upsertSessionOverview(existing, provisionalId, { indexMissing: false });
        setStatus(
          `已加载本地索引（${existing.file_count} 个文件），未重新扫描。需要更新时点「重新索引」。`
        );
        return;
      }

      await runIndexAtPath(path, provisionalId);
    } catch (err) {
      if (indexingId) {
        setSessions((current) =>
          current.map((session) =>
            session.id === indexingId ? { ...session, indexing: false } : session
          )
        );
      }
      setError(err instanceof Error ? err.message : String(err));
      setStatus(null);
    }
  }

  async function openSampleProject() {
    setError(null);
    setStatus("正在准备内置示例项目…");
    let indexingId: string | null = null;
    try {
      const path = await prepareSampleProject();
      const provisionalId = normalizeRootKey(path);
      indexingId = provisionalId;
      const existing = await loadIndexedProject(path);
      if (existing) {
        await upsertSessionOverview(existing, provisionalId, { indexMissing: false });
        setStatus(`已加载内置示例项目（${existing.file_count} 个文件）。`);
        return;
      }
      await runIndexAtPath(path, provisionalId);
    } catch (err) {
      if (indexingId) {
        setSessions((current) =>
          current.map((session) =>
            session.id === indexingId ? { ...session, indexing: false } : session
          )
        );
      }
      setError(err instanceof Error ? err.message : String(err));
      setStatus(null);
    }
  }

  async function reindexActiveProject() {
    if (!active) {
      return;
    }
    setError(null);
    const path = active.overview.root;
    const provisionalId = active.id;
    try {
      await runIndexAtPath(path, provisionalId);
    } catch (err) {
      setSessions((current) =>
        current.map((session) =>
          session.id === provisionalId ? { ...session, indexing: false } : session
        )
      );
      setError(err instanceof Error ? err.message : String(err));
      setStatus(null);
    }
  }

  function closeSession(rootId: string, event?: React.MouseEvent) {
    event?.stopPropagation();
    const index = sessions.findIndex((session) => session.id === rootId);
    if (index < 0) {
      return;
    }
    const closing = sessions[index];
    retainedChatsRef.current[rootId] = {
      chats: closing.chats,
      activeChatId: closing.activeChatId
    };
    const nextSessions = sessions.filter((session) => session.id !== rootId);
    setSessions(nextSessions);
    if (activeRoot === rootId) {
      const fallback = nextSessions[index] ?? nextSessions[index - 1] ?? null;
      setActiveRoot(fallback?.id ?? null);
    }
  }

  function createChatInActiveProject() {
    if (!active) {
      return;
    }
    const chat = createChat(`对话 ${active.chats.length + 1}`);
    setSessions((current) =>
      current.map((session) =>
        session.id === active.id
          ? {
              ...session,
              chats: [chat, ...session.chats],
              activeChatId: chat.id
            }
          : session
      )
    );
  }

  function closeChat(projectId: string, chatId: string, event?: React.MouseEvent) {
    event?.stopPropagation();
    setSessions((current) =>
      current.map((session) => {
        if (session.id !== projectId || session.chats.length <= 1) {
          return session;
        }
        const chats = session.chats.filter((chat) => chat.id !== chatId);
        const activeChatId =
          session.activeChatId === chatId ? chats[0].id : session.activeChatId;
        return { ...session, chats, activeChatId };
      })
    );
  }

  async function runChatAction(
    projectId: string,
    chatId: string,
    label: string,
    action: () => Promise<void>
  ) {
    setError(null);
    setStatus(label);
    patchChat(projectId, chatId, { running: true });
    try {
      await action();
      setStatus(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      setStatus(null);
    } finally {
      patchChat(projectId, chatId, { running: false });
    }
  }

  async function sendQuestion(text: string, graphNodeId?: string) {
    if (!active || !activeChat || !text.trim() || activeChat.running || active.indexMissing) {
      return;
    }
    const root = active.overview.root;
    const projectId = active.overview.project_id;
    const sessionId = active.id;
    const chatId = activeChat.id;
    const trimmed = text.trim();

    await runChatAction(sessionId, chatId, "正在检索代码并生成回答…", async () => {
      const userMessage: ChatMessage = {
        id: createId(),
        role: "user",
        text: trimmed,
        ...(graphNodeId ? { graph_node_id: graphNodeId } : {})
      };
      patchChat(sessionId, chatId, (chat) => ({
        ...chat,
        question: "",
        title: chat.messages.length === 0 ? titleFromText(trimmed) : chat.title,
        messages: [...chat.messages, userMessage]
      }));
      const response: AskResponse = await askProject(root, projectId, trimmed);
      patchChat(sessionId, chatId, (chat) => ({
        ...chat,
        messages: [
          ...chat.messages,
          {
            id: createId(),
            role: "assistant",
            kind: "text",
            text: response.answer,
            citations: response.citations,
            ...(graphNodeId ? { graph_node_id: graphNodeId } : {})
          }
        ]
      }));
    });
  }

  async function sendInspect(text: string) {
    if (!active || !activeChat || !text.trim() || activeChat.running || active.indexMissing) {
      return;
    }
    const root = active.overview.root;
    const projectId = active.overview.project_id;
    const sessionId = active.id;
    const chatId = activeChat.id;
    const query = text.trim();

    await runChatAction(sessionId, chatId, "正在查询符号影响面…", async () => {
      const userMessage: ChatMessage = {
        id: createId(),
        role: "user",
        text: `查符号：${query}`
      };
      patchChat(sessionId, chatId, (chat) => ({
        ...chat,
        question: "",
        composerMode: "symbol",
        title: chat.messages.length === 0 ? titleFromText(query) : chat.title,
        messages: [...chat.messages, userMessage]
      }));
      const response = await inspectSymbol(root, projectId, query, 2);
      const citations =
        response.report?.definitions.map((item) => ({
          file: item.file,
          line: item.line,
          title: `${item.file}:${item.line}`
        })) ??
        response.candidates.slice(0, 8).map((item) => ({
          file: item.file,
          line: item.line,
          title: `${item.file}:${item.line}`
        }));
      patchChat(sessionId, chatId, (chat) => ({
        ...chat,
        messages: [
          ...chat.messages,
          {
            id: createId(),
            role: "assistant",
            kind: "symbol-inspect",
            text: response.message,
            citations,
            inspect: response
          }
        ]
      }));
    });
  }

  function submitComposer(text: string) {
    if (!activeChat) {
      return;
    }
    if ((activeChat.composerMode ?? "ask") === "symbol") {
      void sendInspect(text);
      return;
    }
    void sendQuestion(text);
  }

  async function exportSummary(format: "markdown" | "html") {
    if (!active || active.indexMissing || anyIndexing) return;
    setError(null);
    setStatus("正在生成客户摘要…");
    try {
      const text = active.summary || await exportClientSummary(active.overview.root, active.overview.project_id);
      patchProject(active.id, { summary: text });
      const safeName = active.overview.root.split(/[\\/]/).filter(Boolean).pop() || "project";
      if (format === "markdown") {
        downloadText(text, `${safeName}-cppinsight-summary.md`, "text/markdown;charset=utf-8");
      } else {
        downloadText(summaryAsHtml(text, `${safeName} CppInsight summary`, graphSnapshot?.svg ?? "", graphSnapshot?.view ?? ""), `${safeName}-cppinsight-summary.html`, "text/html;charset=utf-8");
      }
      setStatus(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      setStatus(null);
    }
  }

  const composerMode: ComposerMode = activeChat?.composerMode ?? "ask";
  const chatBusy = Boolean(activeChat?.running);
  const projectIndexing = Boolean(active?.indexing);
  const indexBlocked = Boolean(active?.indexMissing);
  const composerLocked = chatBusy || indexBlocked || projectIndexing;

  async function showDiagnosticPreview() {
    setDiagnosticBusy(true);
    setError(null);
    try {
      const report = await previewDiagnostics();
      setDiagnosticPreview(JSON.stringify(report, null, 2));
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setDiagnosticBusy(false);
    }
  }

  async function saveDiagnosticReport() {
    setDiagnosticBusy(true);
    setError(null);
    try {
      const path = await exportDiagnostics();
      if (path) {
        setStatus(`脱敏诊断已导出：${path}`);
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setDiagnosticBusy(false);
    }
  }

  return (
    <div className="app-shell">
      <header className="topbar">
        <div className="brand">
          <strong>CppInsight AI</strong>
          <span>C/C++ 项目摸底</span>
        </div>
        <div className="topbar-actions">
          <button disabled={anyIndexing} onClick={() => void openProject()}>
            {sessions.length > 0 ? "打开其他项目" : "打开项目"}
          </button>
          {active ? (
            <button
              className="secondary"
              disabled={anyIndexing || projectIndexing}
              title="强制重新扫描并更新本地索引"
              onClick={() => void reindexActiveProject()}
            >
              重新索引
            </button>
          ) : null}
          <button
            className="secondary"
            onClick={() => {
              void (async () => {
                const next = !showSettings;
                setShowSettings(next);
                if (next) {
                  setDiagnosticPreview(null);
                  try {
                    const loaded = await getOpenAiSettings();
                    setAiSettings({
                      base_url: loaded.base_url || DEFAULT_AI_SETTINGS.base_url,
                      api_key: "",
                      model: loaded.model || DEFAULT_AI_SETTINGS.model,
                      api_key_configured: loaded.api_key_configured,
                      clear_api_key: false
                    });
                  } catch {
                    setAiSettings(DEFAULT_AI_SETTINGS);
                  }
                }
              })();
            }}
          >
            设置
          </button>
        </div>
      </header>

      {showSettings ? (
        <section className="settings-card">
          <div className="settings-heading">
            <div>
              <h2>AI 设置</h2>
              <span className="settings-version">CppInsight AI {appVersion}</span>
            </div>
            <a
              className="settings-link"
              href="https://github.com/cluo-fast/cppinsight/issues/new/choose"
              target="_blank"
              rel="noreferrer"
            >
              提交反馈
            </a>
          </div>
          <p>
            类似 Codex / Claude Code：配置 Base URL、API Key、Model，可接官方接口或兼容网关。环境变量
            OPENAI_BASE_URL / OPENAI_API_KEY / OPENAI_MODEL 优先于此处保存的值。
          </p>
          <p className="settings-privacy-note">
            扫描、索引和关系图默认只在本机运行；只有主动提问时才会发送所选上下文。诊断导出会先脱敏预览，不包含源码、绝对路径或密钥。
          </p>
          <label className="settings-field">
            <span>Base URL</span>
            <input
              type="url"
              placeholder="https://api.openai.com/v1"
              value={aiSettings.base_url}
              onChange={(event) =>
                setAiSettings((current) => ({ ...current, base_url: event.target.value }))
              }
            />
          </label>
          <label className="settings-field">
            <span>API Key</span>
            <input
              type="password"
              placeholder={
                aiSettings.api_key_configured
                  ? "已安全保存；留空保持不变"
                  : "sk-... 或网关 Token"
              }
              value={aiSettings.api_key}
              disabled={aiSettings.clear_api_key}
              onChange={(event) =>
                setAiSettings((current) => ({
                  ...current,
                  api_key: event.target.value,
                  clear_api_key: false
                }))
              }
            />
          </label>
          {aiSettings.api_key_configured ? (
            <label className="settings-field">
              <span>已保存密钥</span>
              <span>
                <input
                  type="checkbox"
                  checked={aiSettings.clear_api_key}
                  onChange={(event) =>
                    setAiSettings((current) => ({
                      ...current,
                      clear_api_key: event.target.checked,
                      api_key: event.target.checked ? "" : current.api_key
                    }))
                  }
                />{" "}
                清除操作系统凭据库中的 API Key
              </span>
            </label>
          ) : null}
          <label className="settings-field">
            <span>Model</span>
            <input
              type="text"
              placeholder="gpt-4o-mini"
              value={aiSettings.model}
              onChange={(event) =>
                setAiSettings((current) => ({ ...current, model: event.target.value }))
              }
            />
          </label>
          <div className="settings-row">
            <button
              className="secondary"
              disabled={
                settingsSaving ||
                (!aiSettings.api_key_configured && !aiSettings.api_key.trim())
              }
              onClick={() => {
                void (async () => {
                  setSettingsSaving(true);
                  setError(null);
                  try {
                    await setOpenAiSettings({
                      base_url: aiSettings.base_url.trim() || DEFAULT_AI_SETTINGS.base_url,
                      api_key: aiSettings.api_key.trim() || null,
                      model: aiSettings.model.trim() || DEFAULT_AI_SETTINGS.model,
                      clear_api_key: aiSettings.clear_api_key
                    });
                    setShowSettings(false);
                  } catch (err) {
                    setError(err instanceof Error ? err.message : String(err));
                  } finally {
                    setSettingsSaving(false);
                  }
                })();
              }}
            >
              保存
            </button>
            <button
              className="secondary"
              disabled={diagnosticBusy}
              onClick={() => void showDiagnosticPreview()}
            >
              预览脱敏诊断
            </button>
            <button
              className="secondary"
              disabled={diagnosticBusy || !diagnosticPreview}
              onClick={() => void saveDiagnosticReport()}
            >
              导出诊断 JSON
            </button>
          </div>
          {diagnosticPreview ? (
            <div className="diagnostic-preview">
              <p>仅包含版本、平台和项目聚合计数；不包含源码、路径、聊天内容或 API Key。</p>
              <pre>{diagnosticPreview}</pre>
            </div>
          ) : null}
        </section>
      ) : null}

      {error ? <div className="banner error">{error}</div> : null}
      {status ? <div className="banner">{status}</div> : null}
      {active?.indexMissing ? (
        <div className="banner error">
          本地索引不可用。可点「重新索引」重建，或关闭该项目。
        </div>
      ) : null}

      {sessions.length === 0 ? (
        <section className="welcome">
          <h1>像普通软件一样摸清陌生 C++ 仓库</h1>
          <p>
            点「打开项目」，选择源码目录。若目录下已有{" "}
            <code>.cppinsight/index.sqlite</code>，会直接加载，不会重复全量扫描。
          </p>
          <ol>
            <li>打开客户或同事给你的工程目录</li>
            <li>首次会扫描建索引；之后再打开走本地缓存</li>
            <li>代码有变时可点「重新索引」；提问时可开多条对话并行</li>
          </ol>
          <div className="welcome-actions">
            <button className="primary-lg" disabled={anyIndexing} onClick={() => void openProject()}>
              打开 C/C++ 项目
            </button>
            <button className="secondary primary-lg" disabled={anyIndexing} onClick={() => void openSampleProject()}>
              体验内置示例
            </button>
          </div>
        </section>
      ) : (
        <div className="app-layout">
          <aside className="project-rail">
            <div className="rail-header">
              <span>项目 / 对话</span>
              <button
                className="rail-add"
                disabled={anyIndexing}
                title="打开其他项目"
                onClick={() => void openProject()}
              >
                +
              </button>
            </div>
            <ul className="rail-list">
              {sessions.map((session) => {
                const display = splitDisplayPath(session.overview.root);
                const selected = session.id === activeRoot;
                return (
                  <li key={session.id} className="rail-project">
                    <div className="rail-project-row">
                      <button
                        type="button"
                        className={`rail-item${selected ? " active" : ""}`}
                        title={session.overview.root}
                        onClick={() => setActiveRoot(session.id)}
                      >
                        <span className="rail-name">{display.name}</span>
                        <span className="rail-meta">
                          {session.indexing
                            ? "索引中…"
                            : `${session.overview.file_count} 文件 · ${session.chats.length} 对话`}
                        </span>
                      </button>
                      <button
                        type="button"
                        className="rail-close"
                        title="关闭项目"
                        disabled={session.indexing}
                        onClick={(event) => closeSession(session.id, event)}
                      >
                        ×
                      </button>
                    </div>
                    {selected ? (
                      <ul className="chat-list">
                        <li className="chat-list-actions">
                          <button
                            type="button"
                            className="chat-new"
                            disabled={session.indexing}
                            onClick={() => createChatInActiveProject()}
                          >
                            + 新对话
                          </button>
                        </li>
                        {session.chats.map((chat) => {
                          const chatSelected = chat.id === session.activeChatId;
                          return (
                            <li key={chat.id} className="chat-row">
                              <button
                                type="button"
                                className={`chat-item${chatSelected ? " active" : ""}`}
                                onClick={() => patchProject(session.id, { activeChatId: chat.id })}
                              >
                                <span className="chat-title">
                                  {chat.running ? "… " : ""}
                                  {chat.title}
                                </span>
                              </button>
                              <button
                                type="button"
                                className="rail-close chat-close"
                                title="删除对话"
                                disabled={session.chats.length <= 1 || chat.running}
                                onClick={(event) => closeChat(session.id, chat.id, event)}
                              >
                                ×
                              </button>
                            </li>
                          );
                        })}
                      </ul>
                    ) : null}
                  </li>
                );
              })}
            </ul>
          </aside>

          {overview && active && activeChat ? (
            <div className="workspace">
              <div className="workspace-scroll">
                <section className="project-bar">
                  <div>
                    <div className="label">当前项目</div>
                    <div className="path">{overview.root}</div>
                  </div>
                  <div className="stats">
                    <span>{overview.file_count} 个文件</span>
                    <span>{overview.stats.lines.code} 行代码</span>
                    <span>{overview.symbol_count} 个符号</span>
                    <span title={analysisModeLabel(overview.mode || "heuristic")}>
                      {analysisModeLabel(overview.mode || "heuristic")}
                    </span>
                    <span className="muted">{activeChat.title}</span>
                    <button
                      className="secondary"
                      disabled={projectIndexing || active.indexMissing}
                      onClick={() => void exportSummary("markdown")}
                    >
                      导出 Markdown 摘要
                    </button>
                    <button
                      className="secondary"
                      disabled={projectIndexing || active.indexMissing}
                      onClick={() => void exportSummary("html")}
                    >
                      导出 HTML 摘要
                    </button>
                    <button
                      className={`secondary${showGraph ? " active-action" : ""}`}
                      disabled={projectIndexing || active.indexMissing}
                      onClick={() => setShowGraph((current) => !current)}
                    >
                      {showGraph ? "收起关系图" : "打开关系图"}
                    </button>
                  </div>
                </section>

                {showGraph ? (
                  <GraphWorkbench
                    root={overview.root}
                    projectId={overview.project_id}
                    onStatus={setStatus}
                    onInspectSymbol={(query) => void sendInspect(query)}
                    onAskAi={(question, nodeId) => void sendQuestion(question, nodeId)}
                    onGraphSnapshot={setGraphSnapshot}
                    requestedGraphKind={requestedGraphKind}
                    requestedGraphNodeId={requestedGraphNodeId}
                    graphRequestId={graphRequestId}
                    graphDataVersion={graphDataVersion}
                  />
                ) : null}

                <section className="panel-grid">
                  <article className="panel">
                    <h2>入口候选</h2>
                    {entries.length === 0 ? (
                      <p className="muted">未发现常见入口符号，可直接提问排查。</p>
                    ) : (
                      <ul className="file-list">
                        {entries.map((entry) => {
                          const display = splitDisplayPath(entry.file);
                          return (
                            <li key={`${entry.file}:${entry.line}:${entry.name}`}>
                              <button
                                type="button"
                                className="file-row file-row-button"
                                title={`查符号：${entry.name}（${entry.file}:${entry.line}）`}
                                disabled={chatBusy}
                                onClick={() => void sendInspect(entry.name)}
                              >
                                <div className="file-main">
                                  <span className="file-name">{entry.name}</span>
                                  <span className="file-dir">
                                    {display.name}:{entry.line}
                                  </span>
                                </div>
                                <span className="file-meta">查影响面</span>
                              </button>
                            </li>
                          );
                        })}
                      </ul>
                    )}
                  </article>

                  <article className="panel">
                    <h2>模块地图</h2>
                    <ul className="file-list">
                      {modules.map(([path, count]) => (
                        <li key={path} className="file-row" title={path}>
                          <div className="file-main">
                            <span className="file-name">{path}</span>
                          </div>
                          <span className="file-meta">{count} 文件</span>
                        </li>
                      ))}
                    </ul>
                  </article>

                  <article className="panel">
                    <h2>热点文件</h2>
                    <ul className="file-list">
                      {overview.hotspots.map((item) => {
                        const display = splitDisplayPath(item.path);
                        return (
                          <li key={item.path} className="file-row" title={item.path}>
                            <div className="file-main">
                              <span className="file-name">{display.name}</span>
                              <span className="file-dir">{display.dir}</span>
                            </div>
                            <span className="file-meta">{item.code_lines} 行</span>
                          </li>
                        );
                      })}
                    </ul>
                  </article>
                </section>

                <section className="panel messages-panel">
                  <h2>对话 · {activeChat.title}</h2>
                  <div className="messages">
                    {activeChat.messages.length === 0 ? (
                      <p className="muted">
                        还没有消息。可用底部「问 AI」提问，或切换「查符号」。执行中可切到其他对话，主界面不锁定。
                      </p>
                    ) : (
                      activeChat.messages.map((message) => (
                        <div key={message.id} className={`message ${message.role}`}>
                          <div className="role">
                            {message.role === "user" ? "你" : "CppInsight"}
                          </div>
                          <p>{message.text}</p>
                          {message.role === "assistant" && message.graph_node_id ? (
                            <button
                              type="button"
                              className="linkish"
                              onClick={() => openSymbolGraph(message.graph_node_id)}
                            >
                              回到关系图
                            </button>
                          ) : null}
                          {message.kind === "symbol-inspect" && message.inspect ? (
                            <div className="inspect-card">
                              {message.inspect.report ? (
                                <>
                                  {message.inspect.report.definitions[0]?.stable_id ? (
                                    <div className="inspect-graph-action">
                                      <button
                                        type="button"
                                        className="secondary"
                                        onClick={() => openSymbolGraph(message.inspect?.report?.definitions[0]?.stable_id)}
                                      >
                                        在函数调用图中查看
                                      </button>
                                    </div>
                                  ) : null}
                                  <div className="inspect-section">
                                    <h3>定义</h3>
                                    <ul>
                                      {message.inspect.report.definitions.slice(0, 6).map((item) => (
                                        <li key={`def-${item.file}:${item.line}:${item.name}`}>
                                          <code>
                                            {item.name} · {item.file}:{item.line}
                                          </code>
                                        </li>
                                      ))}
                                    </ul>
                                  </div>
                                  <div className="inspect-section">
                                    <h3>上游调用方</h3>
                                    {message.inspect.report.callers.length === 0 ? (
                                      <p className="muted">未发现调用方</p>
                                    ) : (
                                      <ul>
                                        {message.inspect.report.callers.slice(0, 12).map((item) => (
                                          <li
                                            key={`caller-${item.name}-${item.via_file}:${item.via_line}`}
                                          >
                                            <span className="inspect-candidate-actions">
                                              <button
                                                type="button"
                                                className="linkish"
                                                disabled={chatBusy}
                                                onClick={() => void sendInspect(item.name)}
                                              >
                                                {item.name}
                                              </button>
                                              {item.stable_id ? (
                                                <button
                                                  type="button"
                                                  className="linkish"
                                                  onClick={() => openSymbolGraph(item.stable_id)}
                                                >
                                                  图中查看
                                                </button>
                                              ) : null}
                                            </span>
                                            <span className="muted">
                                              {" "}
                                              @ {item.file}:{item.line}
                                            </span>
                                          </li>
                                        ))}
                                      </ul>
                                    )}
                                  </div>
                                  <div className="inspect-section">
                                    <h3>下游被调</h3>
                                    {message.inspect.report.callees.length === 0 ? (
                                      <p className="muted">未发现被调符号</p>
                                    ) : (
                                      <ul>
                                        {message.inspect.report.callees.slice(0, 12).map((item) => (
                                          <li
                                            key={`callee-${item.name}-${item.via_file}:${item.via_line}`}
                                          >
                                            <span className="inspect-candidate-actions">
                                              <button
                                                type="button"
                                                className="linkish"
                                                disabled={chatBusy}
                                                onClick={() => void sendInspect(item.name)}
                                              >
                                                {item.name}
                                              </button>
                                              {item.stable_id ? (
                                                <button
                                                  type="button"
                                                  className="linkish"
                                                  onClick={() => openSymbolGraph(item.stable_id)}
                                                >
                                                  图中查看
                                                </button>
                                              ) : null}
                                            </span>
                                            <span className="muted">
                                              {" "}
                                              @ {item.file}:{item.line}
                                            </span>
                                          </li>
                                        ))}
                                      </ul>
                                    )}
                                  </div>
                                  <div className="inspect-section">
                                    <h3>入口路径</h3>
                                    {message.inspect.report.entry_paths.length === 0 ? (
                                      <p className="muted">未发现从 main/DllMain 等到达的路径</p>
                                    ) : (
                                      <ul>
                                        {message.inspect.report.entry_paths.map((path) => (
                                          <li
                                            key={`path-${path.entry}-${path.hops
                                              .map((h) => h.name)
                                              .join(">")}`}
                                          >
                                            <code>
                                              {path.hops.map((hop) => hop.name).join(" → ")}
                                            </code>
                                          </li>
                                        ))}
                                      </ul>
                                    )}
                                  </div>
                                  <button
                                    type="button"
                                    className="secondary inspect-explain"
                                    disabled={chatBusy}
                                    onClick={() => {
                                      const prompt = explainImpactPrompt(message.inspect!);
                                      updateActiveChat({
                                        composerMode: "ask",
                                        question: prompt
                                      });
                                      void sendQuestion(prompt);
                                    }}
                                  >
                                    用 AI 解释
                                  </button>
                                </>
                              ) : (
                                <div className="inspect-section">
                                  <h3>候选符号</h3>
                                  <ul>
                                    {message.inspect.candidates.slice(0, 12).map((item) => (
                                      <li key={`cand-${item.file}:${item.line}:${item.name}`}>
                                        <span className="inspect-candidate-actions">
                                          <button
                                            type="button"
                                            className="linkish"
                                            disabled={chatBusy}
                                            onClick={() => void sendInspect(item.name)}
                                          >
                                            {item.name}
                                          </button>
                                          {item.stable_id ? (
                                            <button
                                              type="button"
                                              className="linkish"
                                              onClick={() => openSymbolGraph(item.stable_id)}
                                            >
                                              图中查看
                                            </button>
                                          ) : null}
                                        </span>
                                        <span className="muted">
                                          {" "}
                                          · {item.file}:{item.line}
                                        </span>
                                      </li>
                                    ))}
                                  </ul>
                                </div>
                              )}
                            </div>
                          ) : null}
                          {message.citations?.length && message.kind !== "symbol-inspect" ? (
                            <ul>
                              {message.citations.map((citation) => (
                                <li key={`${message.id}-${citation.title}`}>
                                  <code>{citation.title}</code>
                                </li>
                              ))}
                            </ul>
                          ) : null}
                        </div>
                      ))
                    )}
                    {activeChat.running ? (
                      <p className="muted running-hint">正在处理本对话…可切换到其他对话或项目。</p>
                    ) : null}
                  </div>
                </section>

                <section className="panel">
                  <div className="panel-heading-row">
                    <h2>客户摘要</h2>
                  </div>
                  {active.summary ? (
                    <>
                      <pre>{active.summary}</pre>
                      <div className="summary-graph-links">
                        <span className="muted">按报告洞察打开关系图：</span>
                        <button className="secondary" onClick={() => openReportGraph("module_dependencies")}>模块依赖</button>
                        <button className="secondary" onClick={() => openReportGraph("function_calls")}>函数调用</button>
                        <button className="secondary" onClick={() => openReportGraph("class_relations")}>类关系</button>
                        <button className="secondary" onClick={() => openReportGraph("file_includes")}>文件包含</button>
                        <button className="secondary" onClick={() => openReportGraph("entry_paths")}>入口路径</button>
                      </div>
                    </>
                  ) : (
                    <p className="muted">
                      {active.indexMissing
                        ? "完成索引后将生成本地接手报告。"
                        : "正在生成本地接手报告；若生成失败，可点击上方“导出 Markdown 摘要”重试。"}
                    </p>
                  )}
                </section>
              </div>

              <footer className="composer">
                <div className="composer-modes">
                  <button
                    type="button"
                    className={`mode-chip${composerMode === "ask" ? " active" : ""}`}
                    disabled={composerLocked}
                    onClick={() => updateActiveChat({ composerMode: "ask" })}
                  >
                    问 AI
                  </button>
                  <button
                    type="button"
                    className={`mode-chip${composerMode === "symbol" ? " active" : ""}`}
                    disabled={composerLocked}
                    onClick={() => updateActiveChat({ composerMode: "symbol" })}
                  >
                    查符号
                  </button>
                </div>
                {composerMode === "ask" ? (
                  <div className="chip-row">
                    {SUGGESTED_QUESTIONS.map((item) => (
                      <button
                        key={item}
                        className="chip"
                        disabled={composerLocked}
                        onClick={() => {
                          updateActiveChat({ question: item, composerMode: "ask" });
                          void sendQuestion(item);
                        }}
                      >
                        {item}
                      </button>
                    ))}
                  </div>
                ) : (
                  <p className="composer-hint muted">
                    输入函数/类名查看定义、上游调用、下游被调与入口路径（本地索引，不调用模型）。
                  </p>
                )}
                <div className="ask-box">
                  <textarea
                    value={activeChat.question}
                    onChange={(event) => updateActiveChat({ question: event.target.value })}
                    rows={2}
                    disabled={composerLocked}
                    placeholder={
                      composerMode === "symbol"
                        ? "例如：encryptFile / ProjectScanner…"
                        : "问这个项目任何问题…"
                    }
                    onKeyDown={(event) => {
                      if (event.key === "Enter" && !event.shiftKey) {
                        event.preventDefault();
                        if (!composerLocked && activeChat.question.trim()) {
                          submitComposer(activeChat.question);
                        }
                      }
                    }}
                  />
                  <button
                    disabled={composerLocked || !activeChat.question.trim()}
                    onClick={() => submitComposer(activeChat.question)}
                  >
                    {chatBusy
                      ? "处理中…"
                      : composerMode === "symbol"
                        ? "查询"
                        : "发送"}
                  </button>
                </div>
              </footer>
            </div>
          ) : null}
        </div>
      )}
    </div>
  );
}
