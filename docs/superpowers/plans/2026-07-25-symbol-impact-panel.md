# Symbol Impact Panel (Codex-style) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Codex-style symbol inspect in the chat composer: callers/callees + entry paths as structured thread cards, backed by SQLite graph queries.

**Architecture:** Add `inspect_symbol` Tauri command that resolves a query against `symbols`/`calls`, builds heuristic caller/callee edges, BFS for entry paths, and returns a typed report. Frontend adds composer mode toggle and renders inspect cards in the message list.

**Tech Stack:** Rust/rusqlite (desktop Tauri), React + existing CSS tokens.

## Global Constraints

- Chat-first Codex layout; no graph canvas; no Symbols tab
- Heuristic confidence only; cap fan-out
- Do not break Ask / citations path
- Desktop only for this phase

---

## File Structure

| File | Responsibility |
| --- | --- |
| `desktop/src-tauri/src/lib.rs` | Graph helpers + `inspect_symbol_at` + tests |
| `desktop/src-tauri/src/commands.rs` | `inspect_symbol` command |
| `desktop/src/types.ts` | Inspect response types + message kind |
| `desktop/src/api.ts` | `inspectSymbol` invoke |
| `desktop/src/App.tsx` | Composer mode + card UI + entry click |
| `desktop/src/styles.css` | Inspect card styles |

---

### Task 1: Backend inspect_symbol + tests

**Files:** `desktop/src-tauri/src/lib.rs`, `commands.rs`

- [x] Types: `SymbolHit`, `ImpactNeighbor`, `EntryPath`, `ImpactReport`, `InspectSymbolResponse`
- [x] `inspect_symbol_at(connection, project_id, query, depth)`
- [x] Unit tests: callers/callees + entry path on in-memory DB
- [x] Register Tauri command

### Task 2: Frontend Codex composer + cards

**Files:** `types.ts`, `api.ts`, `App.tsx`, `styles.css`

- [x] `inspectSymbol` API
- [x] Composer modes `ask` | `symbol`
- [x] Render inspect cards; entry candidate click → inspect
- [x] Optional「用 AI 解释」fills Ask mode

### Task 3: Verify

- [x] `cargo test` in `desktop/src-tauri`
