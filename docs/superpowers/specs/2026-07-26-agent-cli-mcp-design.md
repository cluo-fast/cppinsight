# Phase 3A: Agent CLI + MCP

## Decision

Ship **CLI first**, thin **MCP** wrapper second (recommended 3A). Defer 3B incremental/diff.

## CLI (`cppinsight-workbench`)

| Command | Behavior |
| --- | --- |
| `inspect --root <path> <query> [--depth N] [--db <sqlite>]` | JSON `InspectSymbolResponse` on stdout |
| Existing `ask` / `list` / `index` / `summary` | Keep; `--root` defaults DB to `<root>/.cppinsight/index.sqlite` |

## MCP

Stdio server under `tools/mcp-server` that shells out to `cppinsight-workbench`.

Tools:
- `cppinsight_inspect` — `{ projectPath, query, depth? }`
- `cppinsight_ask` — `{ projectPath, question }` (optional convenience)

## Non-goals (3B)

- Incremental reindex orchestration in MCP
- Diff impact reports
