# Workspace Persistence (Open Projects + Chats)

## Goal

App restart restores previously opened projects in the sidebar, with chat threads and messages intact. Users should not need to re-pick directories or lose conversation history.

## Decisions (approved)

| Item | Choice |
| --- | --- |
| Storage location | App data: `%APPDATA%\ai.cppinsight.desktop\workspace.sqlite` (alongside `settings.sqlite`) |
| Scope | Open project list **and** per-project chats/messages (option B) |
| Close (×) | Remove from open workspace only; **keep** chats/messages in DB for that `root_key` |
| Re-open same path | Reload overview from local index + restore chats from workspace DB |
| Index missing | Show project entry as unavailable; do **not** auto full-scan; user may Re-index or Close |

## Non-goals (this iteration)

- Syncing chats with the project tree (no `.cppinsight/chats.*`)
- Cloud sync / export of transcripts
- Persisting in-flight `running` state or status/error banners
- Persisting composer draft text (optional follow-up)

## Data model

```
workspace.sqlite

open_projects(
  root_key TEXT PRIMARY KEY,   -- normalize_root_key(path)
  root_path TEXT NOT NULL,     -- last known display/open path
  sort_order INTEGER NOT NULL,
  opened_at TEXT NOT NULL
)

workspace_meta(
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
)
-- key=active_root_key → current sidebar selection

chats(
  id TEXT PRIMARY KEY,
  root_key TEXT NOT NULL,
  title TEXT NOT NULL,
  composer_mode TEXT NOT NULL,  -- ask | symbol
  sort_order INTEGER NOT NULL,
  updated_at TEXT NOT NULL
)

messages(
  id TEXT PRIMARY KEY,
  chat_id TEXT NOT NULL,
  role TEXT NOT NULL,           -- user | assistant | system (as used by UI)
  content TEXT NOT NULL,        -- JSON or text; inspect payloads as JSON blob
  created_at TEXT NOT NULL,
  sort_order INTEGER NOT NULL
)
```

Notes:

- `root_key` uses the same Windows/Unix normalization as index lookup (`normalize_root_key`).
- Closing × deletes the `open_projects` row only.
- Deleting a chat in UI deletes that chat’s `chats` + `messages` rows (explicit user action).
- Message `content` must round-trip existing `ChatMessage` shapes (plain ask text and inspect cards).

## Runtime behavior

### Startup

1. Open `workspace.sqlite` (create/migrate if needed).
2. Read `open_projects` ordered by `sort_order`.
3. For each entry:
   - Call `load_indexed_project(root_path)`.
   - On hit: build `ProjectSession` with overview + chats/messages for `root_key`.
   - On miss: session with placeholder overview or error flag (`indexMissing: true`); empty or still-loaded chats allowed.
4. Restore `active_root_key` if still in open list; else first open project; else welcome screen.

### Open project (directory picker)

1. Resolve `root_key` / path.
2. Upsert `open_projects`.
3. Prefer `load_indexed_project`; else full index (existing flow).
4. Load chats for `root_key` if any; else create default chat titled `对话 1`.
5. Persist.

### Close project (×)

1. Remove from in-memory sessions and from `open_projects`.
2. Do **not** delete `chats` / `messages` for that `root_key`.
3. Update `active_root_key` if needed.

### Chat mutations

Persist after: create/rename/delete chat, switch active chat, append ask/inspect messages, change composer mode. Use short debounce (~300ms) for bursty updates; flush on window close if feasible.

## Backend API (Tauri)

Suggested commands (names may vary slightly in plan):

- `load_workspace() -> WorkspaceSnapshot`
- `save_workspace(snapshot)` or finer-grained:
  - `upsert_open_project(root_path)`
  - `remove_open_project(root_key)`
  - `set_active_root(root_key | null)`
  - `replace_project_chats(root_key, chats)`

Prefer one load + one save/replace-chats for v1 to keep the frontend simple; refine if payload size becomes an issue.

## Frontend

- On mount: `load_workspace` → populate `sessions` / `activeRoot`.
- Keep existing multi-chat / non-blocking semantics.
- Show a clear banner when `indexMissing` (e.g. path gone or `.cppinsight/index.sqlite` missing).
- Rename default first chat remains `对话 1` (already done).

## Compatibility

- Supersedes “in-memory only” note in `2026-07-26-multi-chat-nonblocking-design.md` for persistence.
- Does not change project-local index layout (`<root>/.cppinsight/index.sqlite`).

## Acceptance

1. Open project A, create chats, ask/inspect, quit app → relaunch shows A and same chats/messages.
2. Close A with ×, quit, relaunch → A not in sidebar; open A again → chats restored from DB.
3. Open A with existing index → no full rescan on restore.
4. Delete index folder, restore workspace → A appears with unavailable state, no auto scan.
5. Multiple open projects restore order and active selection.
