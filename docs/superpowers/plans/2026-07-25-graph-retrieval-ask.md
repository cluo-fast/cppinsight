# Graph-Retrieval Q&A Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make desktop Ask use structure-first retrieval (symbols → defs/refs → call neighbors → code chunks) before LLM, with citations preserved.

**Architecture:** Persist scanner `calls` into project-local SQLite alongside existing `symbols` / `includes` / `chunks`. Enhance `search_contexts` to match question tokens against symbol names, expand to definition sites, call sites, enclosing callers, and related includes, then map those locations onto chunks with high scores. Keep keyword/overview fallback for remaining slots.

**Tech Stack:** Tauri desktop Rust (`desktop/src-tauri/src/lib.rs`), rusqlite, existing Ask/OpenAI path.

## Global Constraints

- Desktop only; do not change C++ scanner call JSON shape.
- Answers must still return `citations` derived from selected contexts.
- No MCP/CLI, no blast-radius UI panel (Phase 2).
- No full LLM knowledge-graph visualization.
- Prefer reusing `chunks` table; do not invent a second snippet store.
- Existing keyword scoring remains as fallback when graph hits are sparse.

---

## File Structure

| File | Responsibility |
| --- | --- |
| `docs/superpowers/plans/2026-07-25-graph-retrieval-ask.md` | This plan |
| `desktop/src-tauri/src/lib.rs` | `calls` schema/write/migrate; graph seed expansion; `search_contexts` integration; unit tests |

---

### Task 1: Persist calls in SQLite

**Files:**
- Modify: `desktop/src-tauri/src/lib.rs`

**Schema:**

```sql
CREATE TABLE IF NOT EXISTS calls (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    project_id INTEGER NOT NULL,
    file TEXT NOT NULL,
    line INTEGER NOT NULL,
    name TEXT NOT NULL,
    confidence TEXT NOT NULL DEFAULT 'heuristic',
    FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_calls_project_name ON calls(project_id, name);
CREATE INDEX IF NOT EXISTS idx_symbols_project_name ON symbols(project_id, name);
```

- [x] **Step 1: Write failing tests** for `write_index` inserting calls and loading by name
- [x] **Step 2: Add table + indexes in `migrate_database`**
- [x] **Step 3: Insert/delete calls in `write_index`; copy in legacy `copy_project_index`**
- [x] **Step 4: Run `cargo test` for call persistence tests**

---

### Task 2: Graph seed → neighbor expansion → chunk selection

**Files:**
- Modify: `desktop/src-tauri/src/lib.rs`

**Interfaces:**

- `fn match_symbol_seeds(connection, project_id, tokens) -> Vec<SymbolInfo>`
- `fn expand_graph_locations(connection, project_id, seeds) -> Vec<(file, line, score_boost)>`
  - defs/decls of seed names
  - call sites where `calls.name` matches seed
  - enclosing caller symbol (nearest prior definition in same file)
  - includes from seed files / resolved_path equals seed file
- `fn chunks_covering(connection, project_id, locations, limit) -> Vec<SearchContext>`
- Integrate into `search_contexts`: graph contexts first, then existing keyword/overview fill

- [x] **Step 1: Failing test** — question containing a symbol name prefers definition + call-site chunks over unrelated keyword noise
- [x] **Step 2: Implement seed match + expansion + chunk cover**
- [x] **Step 3: Wire into `search_contexts` (ask path unchanged at command layer)**
- [x] **Step 4: `cargo test` graph retrieval tests**

---

### Task 3: Verify Ask citations still come from selected contexts

**Files:**
- Modify only if needed: none expected (`ask_project_at_with_settings` already uses `contexts_to_citations`)

- [x] **Step 1: Confirm ask still maps selected contexts → citations**
- [x] **Step 2: Run full `cargo test` in `desktop/src-tauri`**

---

## Done when

1. Re-index writes `calls` rows.
2. Asking about a known symbol retrieves its definition chunk and at least one call-site/neighbor chunk when present.
3. Citations remain non-empty for successful retrieval.
4. Overview / pure-CJK questions still get layout fallback behavior.
