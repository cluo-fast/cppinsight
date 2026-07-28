# Multi-Chat per Project + Non-blocking UI

## Goals

1. One project can host multiple conversations (Codex-style).
2. While one chat is running Ask/Inspect, the main window stays interactive: switch chats/projects, open settings, start another chat.

## Model

```
ProjectSession {
  id, overview, summary, indexing?
  chats: ChatThread[]
  activeChatId
}

ChatThread {
  id, title, messages, question, composerMode, running?
}
```

## Busy semantics

| Action | Lock scope |
| --- | --- |
| Index / open project scan | That project `indexing` only |
| Ask / Inspect | That chat `running` only |
| Settings save | Brief local flag; does not freeze rails |

## Backend

- `inspect_symbol` → async + `spawn_blocking` (same pattern as scan/index).

## Persistence

Persisted via app-data `workspace.sqlite` (open projects + chats). See
`2026-07-27-workspace-persistence-design.md`. Closing a project (×) keeps chats in DB.
