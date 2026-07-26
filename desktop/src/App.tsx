import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import React, { useMemo, useState } from "react";
import {
  askProject,
  chooseProjectDirectory,
  exportClientSummary,
  formatIndexProgress,
  getOpenAiSettings,
  indexProject,
  inspectSymbol,
  setOpenAiSettings
} from "./api";
import type {
  AskResponse,
  ChatMessage,
  ComposerMode,
  IndexProgressEvent,
  InspectSymbolResponse,
  OpenAiSettings,
  ProjectOverview,
  SymbolInfo
} from "./types";
import "./styles.css";

const DEFAULT_AI_SETTINGS: OpenAiSettings = {
  base_url: "https://api.openai.com/v1",
  api_key: "",
  model: "gpt-4o-mini"
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

export default function App() {
  const [sessions, setSessions] = useState<ProjectSession[]>([]);
  const [activeRoot, setActiveRoot] = useState<string | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [aiSettings, setAiSettings] = useState<OpenAiSettings>(DEFAULT_AI_SETTINGS);
  const [showSettings, setShowSettings] = useState(false);
  const [settingsSaving, setSettingsSaving] = useState(false);

  const active = useMemo(
    () => sessions.find((session) => session.id === activeRoot) ?? null,
    [sessions, activeRoot]
  );

  const activeChat = useMemo(() => {
    if (!active) {
      return null;
    }
    return active.chats.find((chat) => chat.id === active.activeChatId) ?? active.chats[0] ?? null;
  }, [active]);

  const overview = active?.overview ?? null;
  const anyIndexing = sessions.some((session) => session.indexing);

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
      setSessions((current) =>
        current.map((session) =>
          session.id === provisionalId ? { ...session, indexing: true } : session
        )
      );
      if (sessions.some((session) => session.id === provisionalId)) {
        setActiveRoot(provisionalId);
      }
      setStatus("正在扫描代码并建立索引…");

      let unlisten: UnlistenFn | undefined;
      try {
        unlisten = await listen<IndexProgressEvent>("index-progress", (event) => {
          setStatus(formatIndexProgress(event.payload));
        });
        const next = await indexProject(path);
        const id = normalizeRootKey(next.root);
        const firstChat = createChat("总览对话");
        setSessions((current) => {
          const existing = current.find(
            (session) => session.id === id || session.id === provisionalId
          );
          if (existing) {
            return current.map((session) =>
              session.id === existing.id
                ? {
                    ...session,
                    id,
                    overview: next,
                    indexing: false,
                    chats: session.chats.length > 0 ? session.chats : [firstChat],
                    activeChatId:
                      session.chats.find((chat) => chat.id === session.activeChatId)?.id ??
                      firstChat.id
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
              chats: [firstChat],
              activeChatId: firstChat.id
            }
          ];
        });
        setActiveRoot(id);
        setStatus(null);
      } finally {
        if (unlisten) {
          unlisten();
        }
      }
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

  function closeSession(rootId: string, event?: React.MouseEvent) {
    event?.stopPropagation();
    const index = sessions.findIndex((session) => session.id === rootId);
    if (index < 0) {
      return;
    }
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

  async function sendQuestion(text: string) {
    if (!active || !activeChat || !text.trim() || activeChat.running) {
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
        text: trimmed
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
            citations: response.citations
          }
        ]
      }));
    });
  }

  async function sendInspect(text: string) {
    if (!active || !activeChat || !text.trim() || activeChat.running) {
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

  const composerMode: ComposerMode = activeChat?.composerMode ?? "ask";
  const chatBusy = Boolean(activeChat?.running);
  const projectIndexing = Boolean(active?.indexing);

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
          <button
            className="secondary"
            onClick={() => {
              void (async () => {
                const next = !showSettings;
                setShowSettings(next);
                if (next) {
                  try {
                    const loaded = await getOpenAiSettings();
                    setAiSettings({
                      base_url: loaded.base_url || DEFAULT_AI_SETTINGS.base_url,
                      api_key: loaded.api_key || "",
                      model: loaded.model || DEFAULT_AI_SETTINGS.model
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
          <h2>AI 设置</h2>
          <p>
            类似 Codex / Claude Code：配置 Base URL、API Key、Model，可接官方接口或兼容网关。环境变量
            OPENAI_BASE_URL / OPENAI_API_KEY / OPENAI_MODEL 优先于此处保存的值。
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
              placeholder="sk-... 或网关 Token"
              value={aiSettings.api_key}
              onChange={(event) =>
                setAiSettings((current) => ({ ...current, api_key: event.target.value }))
              }
            />
          </label>
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
              disabled={settingsSaving || !aiSettings.api_key.trim()}
              onClick={() => {
                void (async () => {
                  setSettingsSaving(true);
                  setError(null);
                  try {
                    await setOpenAiSettings({
                      base_url: aiSettings.base_url.trim() || DEFAULT_AI_SETTINGS.base_url,
                      api_key: aiSettings.api_key.trim(),
                      model: aiSettings.model.trim() || DEFAULT_AI_SETTINGS.model
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
          </div>
        </section>
      ) : null}

      {error ? <div className="banner error">{error}</div> : null}
      {status ? <div className="banner">{status}</div> : null}

      {sessions.length === 0 ? (
        <section className="welcome">
          <h1>像普通软件一样摸清陌生 C++ 仓库</h1>
          <p>
            点「打开项目」，选择源码目录。应用会自动扫描、生成总览，并支持中文提问。可同时打开多个项目，每个项目可开多条对话。
          </p>
          <ol>
            <li>打开客户或同事给你的工程目录</li>
            <li>先看模块地图、入口候选、热点文件</li>
            <li>再问「入口在哪」「某目录负责什么」；执行中可切到其他对话</li>
          </ol>
          <button className="primary-lg" disabled={anyIndexing} onClick={() => void openProject()}>
            打开 C/C++ 项目
          </button>
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
                    <span>{overview.mode || "heuristic"}</span>
                    <span className="muted">{activeChat.title}</span>
                    <button
                      className="secondary"
                      disabled={projectIndexing}
                      onClick={() => {
                        void (async () => {
                          setError(null);
                          setStatus("正在生成客户摘要…");
                          try {
                            const text = await exportClientSummary(
                              overview.root,
                              overview.project_id
                            );
                            patchProject(active.id, { summary: text });
                            setStatus(null);
                          } catch (err) {
                            setError(err instanceof Error ? err.message : String(err));
                            setStatus(null);
                          }
                        })();
                      }}
                    >
                      导出客户摘要
                    </button>
                  </div>
                </section>

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
                          {message.kind === "symbol-inspect" && message.inspect ? (
                            <div className="inspect-card">
                              {message.inspect.report ? (
                                <>
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
                                            <button
                                              type="button"
                                              className="linkish"
                                              disabled={chatBusy}
                                              onClick={() => void sendInspect(item.name)}
                                            >
                                              {item.name}
                                            </button>
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
                                            <button
                                              type="button"
                                              className="linkish"
                                              disabled={chatBusy}
                                              onClick={() => void sendInspect(item.name)}
                                            >
                                              {item.name}
                                            </button>
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
                                        <button
                                          type="button"
                                          className="linkish"
                                          disabled={chatBusy}
                                          onClick={() => void sendInspect(item.name)}
                                        >
                                          {item.name}
                                        </button>
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

                {active.summary ? (
                  <section className="panel">
                    <h2>客户摘要</h2>
                    <pre>{active.summary}</pre>
                  </section>
                ) : null}
              </div>

              <footer className="composer">
                <div className="composer-modes">
                  <button
                    type="button"
                    className={`mode-chip${composerMode === "ask" ? " active" : ""}`}
                    disabled={chatBusy}
                    onClick={() => updateActiveChat({ composerMode: "ask" })}
                  >
                    问 AI
                  </button>
                  <button
                    type="button"
                    className={`mode-chip${composerMode === "symbol" ? " active" : ""}`}
                    disabled={chatBusy}
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
                        disabled={chatBusy}
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
                    placeholder={
                      composerMode === "symbol"
                        ? "例如：encryptFile / ProjectScanner…"
                        : "问这个项目任何问题…"
                    }
                    onKeyDown={(event) => {
                      if (event.key === "Enter" && !event.shiftKey) {
                        event.preventDefault();
                        if (!chatBusy && activeChat.question.trim()) {
                          submitComposer(activeChat.question);
                        }
                      }
                    }}
                  />
                  <button
                    disabled={chatBusy || !activeChat.question.trim()}
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
