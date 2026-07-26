# Symbol Impact & Entry Paths (Codex-style) Design

## Goal

Give unfamiliar C++ projects a Codex-like **structure tool** in the chat workspace: look up a symbol, see blast radius (callers/callees), and entry paths — without leaving the conversation or depending on the LLM.

## Non-Goals

- Force-directed / graph canvas visualization
- Separate IDE-style Symbols tab that replaces chat
- MCP/CLI (Phase 3)
- Compiler-accurate call graphs (heuristic index only; label confidence)

## UX (Codex-aligned)

| Surface | Behavior |
| --- | --- |
| Left rail | Projects (unchanged) |
| Main thread | Chat; structure results appear as assistant **cards** in the thread |
| Composer | Toggle **问 AI** / **查符号**; Enter runs the active mode |
| Overview panels | Keep entry/module/hotspots; clicking an entry name runs **查符号** into the thread |
| Follow-up | Card action **用 AI 解释** fills Ask with a short impact summary |

No new full-page tab. Results stay in the active project session messages.

## Backend

Reuse project-local SQLite (`symbols`, `calls`, `includes`).

### Command

`inspect_symbol(root, project_id, query, depth?) -> InspectSymbolResponse`

- `depth` default `2`, clamp `1..=4`
- Match order: exact name (case-insensitive) → prefix → substring; prefer definitions
- If no hit: return empty candidates + message
- If multiple ambiguous hits and no exact unique name: return `candidates` only
- If unique/exact: return full `report`

### Report fields

- `symbol`, `definitions[]` (`file`, `line`, `kind`, `signature`, `is_definition`)
- `callers[]` / `callees[]`: `{ name, file, line, via_file, via_line }` up to depth, capped (e.g. 40 each)
- `entry_paths[]`: up to 3 paths from known entries (`main`, `wmain`, `WinMain`, `wWinMain`, `DllMain`, `_tmain`, `_tWinMain`) to the target; each path is an ordered list of hops
- `confidence`: `"heuristic"`

### Graph rules (heuristic)

- **Caller of S**: `calls.name = S`, enclosing definition in same file with `line <= call.line` is the caller
- **Callee of S**: calls whose enclosing definition name is `S`
- **Entry path**: BFS backward from target along caller edges until an entry name is reached (max depth 10)

## Frontend data

Extend `ChatMessage` with optional `kind: "text" | "symbol-inspect"` and optional `inspect?: InspectSymbolResponse` for card rendering. Citations list still used for file:line chips when present.

## Success criteria

1. Re-indexed project: querying a known function shows definition + at least one caller or callee when present in index
2. When an entry can reach the symbol via call edges, an entry path is shown
3. Composer mode toggle works; entry panel click uses the same path
4. Ask mode unchanged; no LLM required for inspect
