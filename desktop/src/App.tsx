import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import React, { useMemo, useState } from "react";
import {
  askProject,
  chooseProjectDirectory,
  exportClientSummary,
  formatIndexProgress,
  getOpenAiSettings,
  indexProject,
  setOpenAiSettings
} from "./api";
import type {
  AskResponse,
  ChatMessage,
  IndexProgressEvent,
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

type ProjectSession = {
  id: string;
  overview: ProjectOverview;
  messages: ChatMessage[];
  question: string;
  summary: string;
};

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
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [aiSettings, setAiSettings] = useState<OpenAiSettings>(DEFAULT_AI_SETTINGS);
  const [showSettings, setShowSettings] = useState(false);

  const active = useMemo(
    () => sessions.find((session) => session.id === activeRoot) ?? null,
    [sessions, activeRoot]
  );

  const overview = active?.overview ?? null;

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

  function updateActiveSession(patch: Partial<Omit<ProjectSession, "id">>) {
    if (!activeRoot) {
      return;
    }
    setSessions((current) =>
      current.map((session) =>
        session.id === activeRoot ? { ...session, ...patch } : session
      )
    );
  }

  async function withBusy(label: string, action: () => Promise<void>) {
    setBusy(true);
    setError(null);
    setStatus(label);
    try {
      await action();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
      setStatus(null);
    }
  }

  async function openProject() {
    await withBusy("正在选择并分析项目，请稍候…", async () => {
      const path = await chooseProjectDirectory();
      if (!path) {
        return;
      }

      setStatus("正在扫描代码并建立索引…");
      let unlisten: UnlistenFn | undefined;
      try {
        unlisten = await listen<IndexProgressEvent>("index-progress", (event) => {
          setStatus(formatIndexProgress(event.payload));
        });
        const next = await indexProject(path);
        const id = normalizeRootKey(next.root);
        setSessions((current) => {
          const existing = current.find((session) => session.id === id);
          if (existing) {
            return current.map((session) =>
              session.id === id
                ? {
                    ...session,
                    overview: next
                  }
                : session
            );
          }
          return [
            ...current,
            {
              id,
              overview: next,
              messages: [],
              question: SUGGESTED_QUESTIONS[0],
              summary: ""
            }
          ];
        });
        setActiveRoot(id);
      } finally {
        if (unlisten) {
          unlisten();
        }
      }
    });
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

  async function sendQuestion(text: string) {
    if (!active || !text.trim()) {
      return;
    }
    const root = active.overview.root;
    const projectId = active.overview.project_id;
    const activeId = active.id;
    await withBusy("正在检索代码并生成回答…", async () => {
      const userMessage: ChatMessage = {
        id: createId(),
        role: "user",
        text: text.trim()
      };
      setSessions((current) =>
        current.map((session) =>
          session.id === activeId
            ? { ...session, messages: [...session.messages, userMessage] }
            : session
        )
      );
      const response: AskResponse = await askProject(root, projectId, text.trim());
      setSessions((current) =>
        current.map((session) =>
          session.id === activeId
            ? {
                ...session,
                messages: [
                  ...session.messages,
                  {
                    id: createId(),
                    role: "assistant",
                    text: response.answer,
                    citations: response.citations
                  }
                ]
              }
            : session
        )
      );
    });
  }

  return (
    <div className="app-shell">
      <header className="topbar">
        <div className="brand">
          <strong>CppInsight AI</strong>
          <span>C/C++ 项目摸底</span>
        </div>
        <div className="topbar-actions">
          <button disabled={busy} onClick={() => void openProject()}>
            {sessions.length > 0 ? "打开其他项目" : "打开项目"}
          </button>
          <button
            className="secondary"
            disabled={busy}
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
              disabled={busy || !aiSettings.api_key.trim()}
              onClick={() =>
                void withBusy("正在保存 AI 设置…", async () => {
                  await setOpenAiSettings({
                    base_url: aiSettings.base_url.trim() || DEFAULT_AI_SETTINGS.base_url,
                    api_key: aiSettings.api_key.trim(),
                    model: aiSettings.model.trim() || DEFAULT_AI_SETTINGS.model
                  });
                  setShowSettings(false);
                })
              }
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
            点「打开项目」，选择源码目录。应用会自动扫描、生成总览，并支持中文提问。可同时打开多个项目，从左侧切换。
          </p>
          <ol>
            <li>打开客户或同事给你的工程目录</li>
            <li>先看模块地图、入口候选、热点文件</li>
            <li>再问「入口在哪」「某目录负责什么」</li>
          </ol>
          <button className="primary-lg" disabled={busy} onClick={() => void openProject()}>
            打开 C/C++ 项目
          </button>
        </section>
      ) : (
        <div className="app-layout">
          <aside className="project-rail">
            <div className="rail-header">
              <span>项目</span>
              <button
                className="rail-add"
                disabled={busy}
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
                  <li key={session.id}>
                    <button
                      type="button"
                      className={`rail-item${selected ? " active" : ""}`}
                      title={session.overview.root}
                      disabled={busy}
                      onClick={() => setActiveRoot(session.id)}
                    >
                      <span className="rail-name">{display.name}</span>
                      <span className="rail-meta">{session.overview.file_count} 文件</span>
                    </button>
                    <button
                      type="button"
                      className="rail-close"
                      title="关闭项目"
                      disabled={busy}
                      onClick={(event) => closeSession(session.id, event)}
                    >
                      ×
                    </button>
                  </li>
                );
              })}
            </ul>
          </aside>

          {overview && active ? (
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
                    <button
                      className="secondary"
                      disabled={busy}
                      onClick={() =>
                        void withBusy("正在生成客户摘要…", async () => {
                          const text = await exportClientSummary(
                            overview.root,
                            overview.project_id
                          );
                          updateActiveSession({ summary: text });
                        })
                      }
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
                            <li
                              key={`${entry.file}:${entry.line}:${entry.name}`}
                              className="file-row"
                              title={`${entry.file}:${entry.line}`}
                            >
                              <div className="file-main">
                                <span className="file-name">{entry.name}</span>
                                <span className="file-dir">
                                  {display.name}:{entry.line}
                                </span>
                              </div>
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
                  <h2>对话</h2>
                  <div className="messages">
                    {active.messages.length === 0 ? (
                      <p className="muted">还没有对话。可点下方推荐问题，或直接输入。</p>
                    ) : (
                      active.messages.map((message) => (
                        <div key={message.id} className={`message ${message.role}`}>
                          <div className="role">
                            {message.role === "user" ? "你" : "CppInsight"}
                          </div>
                          <p>{message.text}</p>
                          {message.citations?.length ? (
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
                <div className="chip-row">
                  {SUGGESTED_QUESTIONS.map((item) => (
                    <button
                      key={item}
                      className="chip"
                      disabled={busy}
                      onClick={() => {
                        updateActiveSession({ question: item });
                        void sendQuestion(item);
                      }}
                    >
                      {item}
                    </button>
                  ))}
                </div>
                <div className="ask-box">
                  <textarea
                    value={active.question}
                    onChange={(event) =>
                      updateActiveSession({ question: event.target.value })
                    }
                    rows={2}
                    placeholder="问这个项目任何问题…"
                    onKeyDown={(event) => {
                      if (event.key === "Enter" && !event.shiftKey) {
                        event.preventDefault();
                        if (!busy && active.question.trim()) {
                          void sendQuestion(active.question);
                        }
                      }
                    }}
                  />
                  <button
                    disabled={busy || !active.question.trim()}
                    onClick={() => void sendQuestion(active.question)}
                  >
                    发送
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
