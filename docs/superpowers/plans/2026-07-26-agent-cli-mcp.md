# Agent CLI + MCP Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans or implement inline.

**Goal:** Expose graph inspect to agents via CLI JSON + stdio MCP.

**Architecture:** Reuse `inspect_symbol_at` in `cppinsight-workbench inspect`; Node MCP invokes the binary.

---

### Task 1: Workbench `inspect` + root-local DB default

**Files:** `desktop/src-tauri/src/bin/workbench.rs`

- [x] Add `inspect --root/--project`
- [x] Default `--db` to project-local index when `--root` given
- [x] Pretty JSON stdout

### Task 2: MCP server

**Files:** `tools/mcp-server/*`

- [x] package.json + index.mjs stdio MCP
- [x] tools: inspect (+ ask)
- [x] README with Cursor `mcp.json` snippet

### Task 3: Verify

- [x] `cargo run --bin cppinsight-workbench -- inspect --root <fixture> Foo`
- [x] Document usage in tools README
