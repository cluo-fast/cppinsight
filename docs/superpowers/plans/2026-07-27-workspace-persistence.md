# Workspace Persistence Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Persist open projects and chat history in app-data `workspace.sqlite` so restart restores the sidebar and conversations without rescanning.

**Architecture:** Separate SQLite file next to `settings.sqlite`. Backend exposes `load_workspace` / `save_workspace`. Frontend hydrates on mount and debounced-saves after open/close/chat mutations. Close (×) removes only `open_projects` rows; chats stay keyed by `root_key`.

**Tech Stack:** Rust (rusqlite + serde), Tauri commands, React (`App.tsx` / `api.ts` / `types.ts`).

**Spec:** `docs/superpowers/specs/2026-07-27-workspace-persistence-design.md`

## Global Constraints

- Storage path: `%APPDATA%\ai.cppinsight.desktop\workspace.sqlite` (via `app.path().app_data_dir()`)
- Close × must **not** delete `chats` / `messages`
- Restore must call `load_indexed_project` / `try_load_indexed_project_at`; never auto full-scan on startup
- Do not persist `running`, status banners, or composer draft `question`
- Default first chat title remains `对话 1`
- Do not commit unless the user explicitly asks

## File map

| File | Responsibility |
| --- | --- |
| `desktop/src-tauri/src/workspace.rs` | Schema, load/save snapshot, path helper (new module) |
| `desktop/src-tauri/src/lib.rs` | `mod workspace`; register commands; thin re-exports if needed |
| `desktop/src-tauri/src/commands.rs` | `load_workspace`, `save_workspace` Tauri commands |
| `desktop/src/types.ts` | `WorkspaceSnapshot`, persisted chat/message types, `indexMissing` |
| `desktop/src/api.ts` | `loadWorkspace`, `saveWorkspace` |
| `desktop/src/App.tsx` | Hydrate on mount; persist; close keeps chats; missing-index UI |
| `docs/superpowers/specs/2026-07-26-multi-chat-nonblocking-design.md` | Update Persistence section to point at workspace spec |

---

### Task 1: Workspace SQLite module (schema + round-trip)

**Files:**
- Create: `desktop/src-tauri/src/workspace.rs`
- Modify: `desktop/src-tauri/src/lib.rs` (add `mod workspace;`)
- Test: unit tests inside `workspace.rs`

**Interfaces:**
- Produces:
  - `workspace_db_path(app_data_dir: &Path) -> PathBuf`
  - `open_workspace_db(db_path: &Path) -> Result<Connection, String>`
  - `migrate_workspace(connection: &Connection) -> Result<(), String>`
  - Types: `WorkspaceSnapshot`, `PersistedOpenProject`, `PersistedChat`, `PersistedMessage`
  - `load_workspace_snapshot(db_path: &Path) -> Result<WorkspaceSnapshot, String>`
  - `save_workspace_snapshot(db_path: &Path, snapshot: &WorkspaceSnapshot) -> Result<(), String>`
- Consumes: `crate::normalize_root_key`

- [ ] **Step 1: Add `workspace.rs` with types + migrate + failing round-trip test**

```rust
// desktop/src-tauri/src/workspace.rs
use crate::normalize_root_key;
use chrono::Utc;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub fn workspace_db_path(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("workspace.sqlite")
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceSnapshot {
    pub active_root_key: Option<String>,
    pub open_projects: Vec<PersistedOpenProject>,
    pub chats: Vec<PersistedChat>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PersistedOpenProject {
    pub root_key: String,
    pub root_path: String,
    pub sort_order: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PersistedChat {
    pub id: String,
    pub root_key: String,
    pub title: String,
    pub composer_mode: String,
    pub sort_order: i64,
    pub active: bool,
    pub messages: Vec<PersistedMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PersistedMessage {
    pub id: String,
    pub role: String,
    pub content_json: String,
    pub sort_order: i64,
}

pub fn migrate_workspace(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch(
            "
            PRAGMA foreign_keys = ON;
            CREATE TABLE IF NOT EXISTS open_projects (
                root_key TEXT PRIMARY KEY,
                root_path TEXT NOT NULL,
                sort_order INTEGER NOT NULL,
                opened_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS workspace_meta (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS chats (
                id TEXT PRIMARY KEY,
                root_key TEXT NOT NULL,
                title TEXT NOT NULL,
                composer_mode TEXT NOT NULL,
                sort_order INTEGER NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS messages (
                id TEXT PRIMARY KEY,
                chat_id TEXT NOT NULL,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                created_at TEXT NOT NULL,
                sort_order INTEGER NOT NULL,
                FOREIGN KEY(chat_id) REFERENCES chats(id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_chats_root ON chats(root_key);
            CREATE INDEX IF NOT EXISTS idx_messages_chat ON messages(chat_id);
            ",
        )
        .map_err(|e| e.to_string())
}

pub fn open_workspace_db(db_path: &Path) -> Result<Connection, String> {
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let connection = Connection::open(db_path).map_err(|e| e.to_string())?;
    migrate_workspace(&connection)?;
    Ok(connection)
}

// stubs — implement in Step 3
pub fn load_workspace_snapshot(_db_path: &Path) -> Result<WorkspaceSnapshot, String> {
    Ok(WorkspaceSnapshot {
        active_root_key: None,
        open_projects: vec![],
        chats: vec![],
    })
}

pub fn save_workspace_snapshot(
    _db_path: &Path,
    _snapshot: &WorkspaceSnapshot,
) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_round_trip_keeps_chats_after_open_project_removed() {
        let dir = std::env::temp_dir().join(format!("cppinsight-ws-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let db = workspace_db_path(&dir);

        let root_path = r"D:\repos\demo";
        let root_key = normalize_root_key(root_path);
        let snap = WorkspaceSnapshot {
            active_root_key: Some(root_key.clone()),
            open_projects: vec![PersistedOpenProject {
                root_key: root_key.clone(),
                root_path: root_path.to_string(),
                sort_order: 0,
            }],
            chats: vec![PersistedChat {
                id: "c1".into(),
                root_key: root_key.clone(),
                title: "对话 1".into(),
                composer_mode: "ask".into(),
                sort_order: 0,
                active: true,
                messages: vec![PersistedMessage {
                    id: "m1".into(),
                    role: "user".into(),
                    content_json: r#"{"id":"m1","role":"user","text":"入口在哪","kind":"text"}"#.into(),
                    sort_order: 0,
                }],
            }],
        };
        save_workspace_snapshot(&db, &snap).expect("save");
        let loaded = load_workspace_snapshot(&db).expect("load");
        assert_eq!(loaded.open_projects.len(), 1);
        assert_eq!(loaded.chats[0].messages.len(), 1);

        // Simulate close ×: open list empty, chats retained
        let closed = WorkspaceSnapshot {
            active_root_key: None,
            open_projects: vec![],
            chats: snap.chats.clone(),
        };
        save_workspace_snapshot(&db, &closed).expect("save closed");
        let after = load_workspace_snapshot(&db).expect("load after close");
        assert!(after.open_projects.is_empty());
        assert_eq!(after.chats.len(), 1);
        assert_eq!(after.chats[0].messages[0].id, "m1");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
```

In `lib.rs` near other `mod` / near top of file after existing modules:

```rust
pub mod workspace;
```

(If `lib.rs` has no `mod` section yet, add `pub mod workspace;` before `pub fn run()` / after imports.)

- [ ] **Step 2: Run test — expect FAIL (save/load stubs empty)**

Run:

```powershell
cargo test --manifest-path desktop/src-tauri/Cargo.toml workspace_round_trip -- --nocapture
```

Expected: FAIL (assertion on `open_projects.len()` or messages empty).

- [ ] **Step 3: Implement `save_workspace_snapshot` / `load_workspace_snapshot`**

Semantics for v1 (simple, matches spec “one save”):

- `save_workspace_snapshot`:
  1. Open DB + migrate.
  2. In a transaction:
     - `DELETE FROM open_projects`
     - `DELETE FROM workspace_meta WHERE key = 'active_root_key'`
     - Insert each `open_projects` row (`opened_at = Utc::now().to_rfc3339()`).
     - Upsert `active_root_key` meta if `Some`.
     - For chats: **replace all chats whose `root_key` appears in `snapshot.chats` OR in `snapshot.open_projects`**, and also replace any root_key present in the incoming `chats` array.
     - Practical rule used by UI: frontend always sends **full chat set for every `root_key` that still has chats to keep**, including closed projects’ chats when saving after close.
     - Implementation: collect all `root_key`s from `snapshot.chats`; for each such key `DELETE FROM chats WHERE root_key = ?` (CASCADE messages); re-insert chats + messages.
     - Do **not** `DELETE FROM chats` globally — only touch keys present in the snapshot’s `chats` list. When closing a project, frontend still includes that root’s chats in `snapshot.chats` while omitting it from `open_projects`.
  3. Commit.

- `load_workspace_snapshot`:
  - Read `open_projects` ordered by `sort_order`.
  - Read `active_root_key` from meta.
  - Read **all** `chats` + messages (needed so re-open can restore). Group messages by `chat_id`. Set `PersistedChat.active` from a per-root meta key `active_chat:{root_key}` stored in `workspace_meta`, OR store `active` by writing meta `active_chat:{root_key}=chat_id` during save from the chat marked `active: true`.

Add to save: for each distinct `root_key` in chats, write `workspace_meta` key `active_chat:{root_key}` = id of the chat with `active == true` (if any).

- [ ] **Step 4: Re-run test — expect PASS**

```powershell
cargo test --manifest-path desktop/src-tauri/Cargo.toml workspace_round_trip -- --nocapture
```

Expected: `ok`

- [ ] **Step 5: Commit only if user asked** — otherwise skip.

---

### Task 2: Tauri commands + wire `AppHandle` path

**Files:**
- Modify: `desktop/src-tauri/src/commands.rs`
- Modify: `desktop/src-tauri/src/lib.rs` (`invoke_handler` list)

**Interfaces:**
- Consumes: `workspace::*`, `app_data_dir_from_app`, `try_load_indexed_project_at`, `project_index_db_path`
- Produces:
  - `load_workspace(app) -> Result<LoadedWorkspace, String>`
  - `save_workspace(app, snapshot: WorkspaceSnapshot) -> Result<(), String>`

`LoadedWorkspace` (serde, returned to UI):

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadedWorkspace {
    pub active_root_key: Option<String>,
    pub projects: Vec<LoadedProject>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadedProject {
    pub root_key: String,
    pub root_path: String,
    pub index_missing: bool,
    pub overview: Option<crate::ProjectOverview>,
    pub chats: Vec<PersistedChat>,
    pub active_chat_id: Option<String>,
}
```

- [ ] **Step 1: Implement `load_workspace` command**

```rust
#[tauri::command]
pub fn load_workspace(app: tauri::AppHandle) -> Result<LoadedWorkspace, String> {
    let app_data = crate::app_data_dir_from_app(&app)?;
    let db = crate::workspace::workspace_db_path(&app_data);
    let snap = crate::workspace::load_workspace_snapshot(&db)?;

    let mut projects = Vec::new();
    for open in snap.open_projects {
        let root = PathBuf::from(&open.root_path);
        let index_db = crate::project_index_db_path(&root);
        let overview = crate::try_load_indexed_project_at(&root, &index_db)?;
        let index_missing = overview.is_none();
        let chats: Vec<_> = snap
            .chats
            .iter()
            .filter(|c| c.root_key == open.root_key)
            .cloned()
            .collect();
        let active_chat_id = chats.iter().find(|c| c.active).map(|c| c.id.clone())
            .or_else(|| chats.first().map(|c| c.id.clone()));
        projects.push(LoadedProject {
            root_key: open.root_key,
            root_path: open.root_path,
            index_missing,
            overview,
            chats,
            active_chat_id,
        });
    }

    Ok(LoadedWorkspace {
        active_root_key: snap.active_root_key,
        projects,
    })
}
```

- [ ] **Step 2: Implement `save_workspace` command**

```rust
#[tauri::command]
pub fn save_workspace(
    app: tauri::AppHandle,
    snapshot: crate::workspace::WorkspaceSnapshot,
) -> Result<(), String> {
    let app_data = crate::app_data_dir_from_app(&app)?;
    let db = crate::workspace::workspace_db_path(&app_data);
    crate::workspace::save_workspace_snapshot(&db, &snapshot)
}
```

Register both in `invoke_handler`.

- [ ] **Step 3: Smoke-compile**

```powershell
cargo check --manifest-path desktop/src-tauri/Cargo.toml
```

Expected: success (warnings OK).

---

### Task 3: Frontend types + API

**Files:**
- Modify: `desktop/src/types.ts`
- Modify: `desktop/src/api.ts`

- [ ] **Step 1: Add types**

```ts
export interface PersistedMessage {
  id: string;
  role: string;
  content_json: string;
  sort_order: number;
}

export interface PersistedChat {
  id: string;
  root_key: string;
  title: string;
  composer_mode: string;
  sort_order: number;
  active: boolean;
  messages: PersistedMessage[];
}

export interface WorkspaceSnapshot {
  active_root_key: string | null;
  open_projects: { root_key: string; root_path: string; sort_order: number }[];
  chats: PersistedChat[];
}

export interface LoadedProject {
  root_key: string;
  root_path: string;
  index_missing: boolean;
  overview: ProjectOverview | null;
  chats: PersistedChat[];
  active_chat_id: string | null;
}

export interface LoadedWorkspace {
  active_root_key: string | null;
  projects: LoadedProject[];
}
```

- [ ] **Step 2: Add API helpers**

```ts
export function loadWorkspace(): Promise<LoadedWorkspace> {
  return invoke<LoadedWorkspace>("load_workspace");
}

export function saveWorkspace(snapshot: WorkspaceSnapshot): Promise<void> {
  return invoke<void>("save_workspace", { snapshot });
}
```

Also add helpers in `App.tsx` or a small `workspaceCodec.ts` (prefer keep in `App.tsx` if short):

```ts
function messageToPersisted(m: ChatMessage, sort_order: number): PersistedMessage {
  return {
    id: m.id,
    role: m.role,
    content_json: JSON.stringify(m),
    sort_order
  };
}

function messageFromPersisted(p: PersistedMessage): ChatMessage {
  return JSON.parse(p.content_json) as ChatMessage;
}
```

---

### Task 4: Wire `App.tsx` — hydrate, persist, close, missing index

**Files:**
- Modify: `desktop/src/App.tsx`
- Modify: `docs/superpowers/specs/2026-07-26-multi-chat-nonblocking-design.md` (Persistence blurb)

**Interfaces:**
- Consumes: `loadWorkspace`, `saveWorkspace`, codecs above
- Extends `ProjectSession` with `indexMissing?: boolean`

- [ ] **Step 1: Extend session type**

```ts
type ProjectSession = {
  id: string;
  overview: ProjectOverview;
  summary: string;
  indexing: boolean;
  indexMissing?: boolean;
  chats: ChatThread[];
  activeChatId: string;
};
```

Placeholder overview when index missing:

```ts
function placeholderOverview(rootPath: string): ProjectOverview {
  return {
    project_id: -1,
    root: rootPath,
    indexed_at: "",
    file_count: 0,
    symbol_count: 0,
    include_count: 0,
    stats: { source_files: 0, header_files: 0, inline_files: 0, total_bytes: 0, lines: { total: 0, code: 0, comment: 0, blank: 0 } },
    extensions: {},
    top_level_layout: {},
    hotspots: [],
    symbols: [],
    diagnostics: ["本地索引不可用：未找到 .cppinsight/index.sqlite 或项目记录"]
  };
}
```

- [ ] **Step 2: `buildSnapshot(sessions, activeRoot, retainedChatsByRoot)`**

Keep a `Ref` or state map `retainedChats: Record<rootKey, ChatThread[]>` updated on close so closed projects’ chats still enter `saveWorkspace`.

On `closeSession`:
1. Before removing session, copy `session.chats` into `retainedChats[session.id]` (id is normalizeRootKey / root_key).
2. Remove from `sessions` / update `activeRoot` (existing logic).
3. Call `persistWorkspace()`.

On open of path whose `root_key` is in retained or loaded from DB: merge those chats instead of only `对话 1`.

- [ ] **Step 3: `persistWorkspace` debounced 300ms**

Build `WorkspaceSnapshot`:
- `open_projects` from current `sessions` (`root_key = session.id`, `root_path = overview.root`, `sort_order = index`)
- `active_root_key = activeRoot`
- `chats` = flatten chats from **sessions + retainedChats** (for keys not in sessions). Mark `active` from `activeChatId`. Serialize messages via `content_json`. Force `running: false` / omit by stripping before serialize (messages only).

- [ ] **Step 4: `useEffect` on mount → `loadWorkspace`**

Map each `LoadedProject` → `ProjectSession`:
- chats from persisted messages
- `indexMissing` flag
- overview from loaded or placeholder
- skip ask/inspect when `indexMissing || project_id < 0` (disable composer / show banner)

Set `activeRoot` from snapshot if valid.

- [ ] **Step 5: Call persist after** open success, close session, create/close chat, ask/inspect message append, active chat switch, active project switch.

- [ ] **Step 6: UI banner** when `active?.indexMissing`:

```tsx
{active?.indexMissing ? (
  <div className="banner error">
    本地索引不可用。可点「重新索引」重建，或关闭该项目。
  </div>
) : null}
```

Disable Ask/Inspect when missing; allow Re-index and Close.

- [ ] **Step 7: Update multi-chat design Persistence section**

Replace in-memory note with: “Persisted via `workspace.sqlite` — see `2026-07-27-workspace-persistence-design.md`.”

---

### Task 5: Manual acceptance + rebuild

- [ ] **Step 1: Rebuild desktop**

```powershell
Get-Process -Name cppinsight-ai -ErrorAction SilentlyContinue | Stop-Process -Force
$env:CPPINSIGHT_REBUILD = "1"
.\scripts\start-desktop.ps1
```

- [ ] **Step 2: Acceptance checklist (from spec)**

1. Open project A, create 2 chats, send one ask or inspect, quit → relaunch → A + chats/messages present, no full scan.
2. Close A with ×, quit, relaunch → A absent; open A again → chats restored.
3. Existing index → status shows loaded-from-cache style message (or silent restore), not long scan.
4. Rename/move away `.cppinsight` temporarily → restore shows unavailable banner, no auto scan.
5. Two projects open → order + active selection restored.

- [ ] **Step 3: Unit tests still green**

```powershell
cargo test --manifest-path desktop/src-tauri/Cargo.toml workspace_ -- --nocapture
```

---

## Spec coverage (self-review)

| Spec requirement | Task |
| --- | --- |
| `workspace.sqlite` in app data | 1–2 |
| open projects + chats/messages | 1, 4 |
| × keeps chats | 1 test + 4 close/retain |
| restore via load index, no auto scan | 2 `load_workspace` + 4 |
| index missing UI | 4 |
| debounce save | 4 |
| Acceptance 1–5 | 5 |

No intentional placeholders left in task steps.
