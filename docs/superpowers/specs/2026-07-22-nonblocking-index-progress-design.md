# Non-blocking Index Build with Progress

Date: 2026-07-22  
Status: Approved for planning  
Scope: Desktop (Tauri + React) only; C++ CLI unchanged

## Problem

Opening a project runs `index_project`, which currently:

1. Invokes the `cppinsight` CLI via a synchronous Tauri command
2. Waits on `Command::output()` for the full scan
3. Writes the scan into SQLite and returns a `ProjectOverview`

On Windows this makes the main window unresponsive (cannot drag; title bar may show “未响应”). Users also get no file-level progress because the desktop path passes `--quiet`.

## Goals

- Keep the main window responsive while indexing (drag, scroll, title bar responsive).
- Show live scan progress in the existing status banner.
- Keep conflict actions disabled during indexing (open another project / ask / export) to avoid concurrent index jobs.

## Non-goals

- Cancel in-flight indexing.
- Percentage progress bar (CLI reports discovered/analyzed counts, not a total).
- Changes to the C++ scanner core.
- Full E2E automation for the desktop UI.

## Chosen approach

Background execution + parse existing CLI stderr progress (Approach 2).

Alternatives considered:

1. **Background only, stage messages** — fixes freeze, but large repos still feel stuck.
2. **Background + CLI stderr progress (chosen)** — reuses CLI output, focused desktop changes.
3. **Embed scan in Rust with fine-grained callbacks** — best control, far larger scope.

## Architecture

```text
Frontend openProject
  → choose directory (dialog)
  → listen("index-progress")
  → invoke("index_project")   // async command

Rust index_project (async)
  → spawn_blocking {
        start cppinsight scan without --quiet
        stream stderr lines → AppHandle.emit("index-progress", payload)
        parse scan JSON → write_index → overview
        emit stage "indexing" then "done"
     }
  → return ProjectOverview

Frontend
  → update status banner from events
  → unlisten on settle
  → render overview
```

## Components

### Rust (`desktop/src-tauri`)

- Convert `index_project` (and `scan_project` for consistency) to `async`.
- Run blocking work in `tokio::task::spawn_blocking`.
- Change `run_scan` to spawn with piped stderr instead of `Command::output()`.
- Parse CLI progress lines of the form:

  ```text
  [scan] files=1200 analyzed=800 reused=100 current=src/foo.cpp
  ```

- Emit event `index-progress` with payload:

  | Field | Type | Notes |
  | --- | --- | --- |
  | `stage` | `"scanning" \| "indexing" \| "done"` | Required |
  | `files_discovered` | `u64?` | From `files=` |
  | `files_analyzed` | `u64?` | From `analyzed=` |
  | `files_reused` | `u64?` | From `reused=` |
  | `current_file` | `string?` | From `current=` |
  | `message` | `string?` | Human-readable fallback |

- Malformed progress lines are ignored; they must not fail the scan.
- Non-zero CLI exit still returns `Err` to the invoke Promise.

### Frontend (`desktop/src`)

- Before `indexProject`, subscribe to `index-progress`; always unlisten in `finally`.
- Map events into the existing `status` banner, e.g.:
  - scanning: `正在扫描… 已发现 1200 个文件，分析 800（src/foo.cpp）`
  - indexing: `正在写入索引…`
- Keep `busy` gating for conflicting actions.
- Do not add a new progress panel.

## Error handling

- CLI missing / non-zero exit / JSON parse failure → reject invoke; show error banner; clear progress listener.
- Event emit failures during progress are best-effort; indexing continues.
- Partial or missing progress updates must never leave the UI stuck in `busy` after the invoke settles.

## Testing

### Automated

- Unit-test stderr progress parsing:
  - valid line → extracted fields
  - invalid line → `None`, no panic

### Manual acceptance

1. Index a medium/large C++ repo.
2. During scan: window remains draggable; no “未响应”.
3. Status banner updates with rising file counts.
4. On completion: overview and ask flow still work.
5. Bad path / broken CLI: error banner only; UI stays responsive.

## Success criteria

- Indexing never freezes the main window.
- Users see updating scan counts during indexing.
- Existing open → overview → ask path remains functional.
