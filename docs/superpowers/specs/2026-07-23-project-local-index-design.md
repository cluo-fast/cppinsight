# Project-Local Index Storage with Legacy Migration

Date: 2026-07-23  
Status: Approved for planning  
Scope: Desktop (Tauri) index path + settings split + one-time per-project migration

## Problem

Index data currently lives in a global SQLite under the user app-data directory:

`%APPDATA%\ai.cppinsight.desktop\projects.sqlite`

That couples unrelated projects into one file, does not travel with the repo, and is a poor default for large codebases.

## Goals

- Store each project's index under that project's root.
- Keep OpenAI API key out of the project tree.
- Migrate existing per-project rows from the legacy global DB when a project is opened/indexed.
- Preserve non-blocking index progress behavior already shipped.

## Non-goals

- Migrating every project in the legacy DB eagerly on startup.
- Changing the C++ scanner.
- Automatic deletion of the legacy DB file after partial migration (optional cleanup only when empty).

## Chosen layout

| Data | Path |
| --- | --- |
| Project index | `<project-root>/.cppinsight/index.sqlite` |
| App settings (API key) | `%APPDATA%\ai.cppinsight.desktop\settings.sqlite` |
| Override (debug) | `CPPINSIGHT_DB` still forces a single DB path when set (dev/workbench escape hatch) |

Repo `.gitignore` already ignores `**/.cppinsight/`.

## Architecture

```text
index_project(path)
  → scan
  → ensure_project_db(root)          # <root>/.cppinsight/index.sqlite
  → maybe_migrate_from_legacy(root)  # if local empty and legacy has root
  → write_index / or use migrated data
  → return overview (includes root)

ask_project(root, project_id, question)
  → open <root>/.cppinsight/index.sqlite
  → load settings API key from settings.sqlite
  → answer

set_openai_api_key
  → settings.sqlite only
```

## Migration rules

**Legacy candidates (first existing wins for lookup):**

1. `%APPDATA%\ai.cppinsight.desktop\projects.sqlite`
2. `%USERPROFILE%\.cppinsight\projects.sqlite` (older CLI/workbench default)

**Trigger:** On open/index of `root`, if local index DB is missing or has no `projects` row for that root, and a legacy DB contains a matching `root` row.

**Root matching:** Compare normalized absolute paths (case-insensitive on Windows; `/` vs `\` normalized).

**Copied tables for that project only:** `projects`, `files`, `symbols`, `includes`, `chunks`.

**After successful copy:** Delete the project row from the legacy DB (CASCADE clears children).

**API key:** If legacy `settings.openai_api_key` exists and settings.sqlite does not yet have a key, copy once. Never overwrite a newer key in settings.sqlite.

**Failure:** Leave legacy DB unchanged; fall back to fresh scan + write into the project-local DB. Surface a diagnostic/warning if useful; do not crash indexing.

**Empty legacy file:** If legacy DB has no remaining projects, it may be left in place (optional later cleanup). Do not delete while other projects remain.

## Command / API changes

- `index_project` order: scan → ensure local DB dir → **migrate from legacy if needed** → `write_index` (upsert). Current UI always rescans on open; migration still moves the legacy project out of AppData into the project tree before the new write.
- `ask_project` / `export_client_summary`: accept `root` (keep `project_id`) and open `<root>/.cppinsight/index.sqlite`.
- Frontend: pass `overview.root` into ask/export APIs.
- `set_openai_api_key` / key reads: settings.sqlite only.

## Error handling

- Cannot create `.cppinsight/`: fail index with a clear path-permission error.
- Legacy locked/corrupt: skip migrate, continue with fresh index.
- `CPPINSIGHT_DB` set: use that single DB for index (legacy behavior for tooling); settings remain separate unless explicitly documented otherwise — for this change, when `CPPINSIGHT_DB` is set, index commands use it and migration is skipped.

## Testing

- Unit/integration: path helper returns `<root>/.cppinsight/index.sqlite`.
- Migration: seed legacy DB with one project + children + API key; open root → local DB populated; legacy project row removed; key present in settings.sqlite; second run does not duplicate.
- Key isolation: setting key does not create/write project-local settings for secrets.
- Manual: index a project, confirm file appears under project `.cppinsight/`, ask still works.

## Success criteria

- New indexes land under the project, not AppData `projects.sqlite`.
- Existing AppData projects migrate on first open/index of that root.
- API key remains in user app-data settings store.
