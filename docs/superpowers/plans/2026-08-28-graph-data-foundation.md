# Graph Data Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the stable, versioned local graph-data and query foundation that can serve all five launch graph types from one SQLite index through Rust, Tauri, CLI, and MCP interfaces.

**Architecture:** Extend the C++ scan contract with deterministic entity keys, qualified ownership, entry points, and evidence-bearing type/call facts while keeping schema-version-2 input readable. Move new index and graph responsibilities into focused Rust modules: the index module owns migrations, normalization, relation resolution, complete-generation commits, and saved layouts; the graph module owns the versioned request/response model and deterministic bounded traversal. Existing report and AI paths continue to use the legacy public functions until their later dedicated phases, but new consumers call only the graph service rather than deriving relations from raw tables.

**Tech Stack:** C++17 scanner and JSON writer, Rust 2021 with `rusqlite`/`serde`, SQLite, Tauri 2 commands, TypeScript, Node.js MCP SDK.

**Spec:** `docs/superpowers/specs/2026-08-27-public-beta-code-graph-design.md`

## Global Constraints

- Keep Windows as the first desktop release target; keep CLI and MCP path handling portable.
- Use SQLite only; do not introduce a graph database.
- Preserve source evidence and confidence on every emitted or resolved relation.
- Mark unresolved and ambiguous calls or type relations explicitly; never promote them to certain edges.
- Keep old scan JSON readable and add defaults for every new scan-contract field.
- A failed index transaction must leave the previous complete project generation queryable.
- Protocol version is exactly `1`; scan JSON schema version becomes exactly `3`.
- Default graph response limit is `500` nodes; valid requests accept `1..=500`, and larger values return a validation error.
- Graph output ordering is deterministic by stable node ID and stable edge ID.
- Graph and indexing logic remains fully local and must not require AI settings or network access.
- This plan does not add Cytoscape.js or graph-screen UI; those belong to the next phase plan.

---

## File Structure

| File | Responsibility |
| --- | --- |
| `src/project/FileInfo.h` | C++ stable symbol/call/relation/entry fact types |
| `src/project/ProjectScanner.cpp` | Qualified ownership, entry detection, inheritance/composition/use facts, call ownership |
| `src/project/ScanCache.cpp` | Schema-3 incremental-cache round trip |
| `src/report/JsonReportWriter.cpp` | Versioned scan JSON serialization |
| `tests/ProjectScannerTests.cpp` | Scanner contract and stable-identity regression coverage |
| `desktop/src-tauri/src/index.rs` | SQLite migration, stable-key fallback, generation-safe writes, relation resolution and aggregation |
| `desktop/src-tauri/src/graph.rs` | Graph protocol, bounded traversal, filters, paths, cycles, centrality and continuation |
| `desktop/src-tauri/src/lib.rs` | Shared scan/domain types and compatibility re-exports |
| `desktop/src-tauri/src/commands.rs` | Async Tauri graph query and layout commands |
| `desktop/src-tauri/src/bin/workbench.rs` | JSON graph-query CLI transport |
| `desktop/src/types.ts` | TypeScript mirror of protocol version 1 |
| `desktop/src/api.ts` | Typed Tauri graph APIs |
| `tools/mcp-server/index.mjs` | MCP graph-query endpoint forwarding the same request model |
| `tools/mcp-server/test/index.test.mjs` | MCP input validation and CLI forwarding tests |
| `desktop/src-tauri/tests/graph_protocol.rs` | Public protocol serialization and query integration tests |
| `docs/graph-protocol-v1.md` | Public request, response, node-ID and continuation contract |

---

### Task 1: C++ Scan Schema 3 and Stable Analysis Facts

**Files:**
- Modify: `src/project/FileInfo.h`
- Modify: `src/project/ProjectScanner.h`
- Modify: `src/project/ProjectScanner.cpp`
- Modify: `src/project/ScanCache.cpp`
- Modify: `src/report/JsonReportWriter.cpp`
- Test: `tests/ProjectScannerTests.cpp`

**Interfaces:**
- Produces: `SymbolInfo::stableKey`, `qualifiedName`, `parentStableKey`, and `parentQualifiedName`.
- Produces: `CallInfo::callerStableKey`, `callerQualifiedName`, `targetQualifiedName`, and evidence in existing `filePath`/`line` fields.
- Produces: `SymbolRelationInfo { sourceStableKey, sourceQualifiedName, targetName, kind, filePath, line, confidence }` where `kind` is one of `inherits`, `implements`, `contains`, or `uses`.
- Produces: `EntryPointInfo { symbolStableKey, qualifiedName, kind, filePath, line, confidence }` where `kind` is `program`, `library`, or `test`.
- Stable-key canonical input: `kind + "\n" + normalized_file + "\n" + qualified_name + "\n" + normalized_signature`; ID format is `sym:` followed by 16 lowercase hexadecimal FNV-1a-64 digits.
- Consumes later: Rust `ScanResult` deserialization and normalization in Task 3.

- [x] **Step 1: Write failing scanner ownership and stable-key tests**

Add a fixture containing a namespace, base type, interface-like pure-virtual type, composed field, overloaded methods, a multi-line function body, and `main`. Assert:

```cpp
const SymbolInfo* runner = findQualifiedSymbol(result, "app::Runner");
require(runner != nullptr, "Runner should have a qualified name");
require(runner->stableKey.rfind("sym:", 0) == 0, "symbol should have stable key");
require(runner->stableKey.size() == 20, "stable key should contain 16 hex digits");
require(hasRelation(result, "app::Runner", "app::Base", "inherits"),
        "class base should emit inheritance evidence");
require(hasRelation(result, "app::Runner", "app::Widget", "contains"),
        "field should emit composition evidence");
require(hasOwnedCall(result, "app::Runner::run", "helper"),
        "call should retain its enclosing function");
require(hasEntry(result, "main", "program"), "main should be an entry point fact");
```

Scan the same unchanged fixture twice and assert every `(qualifiedName, stableKey)` pair is identical. Add one blank line before the symbols, scan again, and assert stable keys remain identical because line numbers are not part of the canonical key.

- [x] **Step 2: Run the C++ test to verify the new contract fails**

Run:

```bash
cmake -S . -B build-graph-data -DBUILD_TESTING=ON
cmake --build build-graph-data --target cppinsight_tests -j2
ctest --test-dir build-graph-data --output-on-failure
```

Expected: compilation fails because the schema-3 fields and helper fact types do not exist.

- [x] **Step 3: Add exact schema-3 fact types and stable-key helper**

Add the following fields and facts in `FileInfo.h`:

```cpp
struct SymbolInfo {
    std::filesystem::path filePath;
    std::uint64_t line = 0;
    std::string kind;
    std::string name;
    std::string qualifiedName;
    std::string stableKey;
    std::string parentQualifiedName;
    std::string parentStableKey;
    std::string signature;
    std::string scope;
    bool isDefinition = false;
    std::string confidence = "heuristic";
};

struct CallInfo {
    std::filesystem::path filePath;
    std::uint64_t line = 0;
    std::string name;
    std::string targetQualifiedName;
    std::string callerQualifiedName;
    std::string callerStableKey;
    std::string confidence = "heuristic";
};

struct SymbolRelationInfo {
    std::filesystem::path filePath;
    std::uint64_t line = 0;
    std::string sourceQualifiedName;
    std::string sourceStableKey;
    std::string targetName;
    std::string kind;
    std::string confidence = "heuristic";
};

struct EntryPointInfo {
    std::filesystem::path filePath;
    std::uint64_t line = 0;
    std::string qualifiedName;
    std::string symbolStableKey;
    std::string kind;
    std::string confidence = "heuristic";
};
```

Add `relations` and `entryPoints` to `FileInfo` and `ScanResult`. Implement a private FNV-1a helper that hashes the exact canonical string, normalizes `\\` to `/`, collapses whitespace in signatures, and emits lowercase fixed-width hex.

- [x] **Step 4: Track namespace/type/function ownership and relation evidence**

Use brace-depth scopes so calls on lines after a function declaration keep the enclosing function until its closing brace. Build qualified names as:

```text
namespace::Type
namespace::Type::method
namespace::free_function
```

Parse class base lists from `class Name : public Base, virtual Interface {` and emit `inherits` for concrete bases and `implements` when the unqualified base name starts with `I` followed by an uppercase character or the base declaration contains `= 0`. Parse class-scope field declarations ending in `;` and emit `contains` for value members and `uses` for pointer/reference members. Emit `uses` for type names in function signatures. Keep every heuristic relation at `confidence = "heuristic"`.

- [x] **Step 5: Serialize and cache every new field**

Change the JSON header to:

```json
{
  "schema_version": 3,
  "generator_version": "cppinsight-core/0.1.0"
}
```

Serialize top-level and per-file `relations` and `entry_points`. Update `ScanCache.cpp` readers so missing keys produce empty strings/vectors, allowing schema-2 cache files to remain readable.

- [x] **Step 6: Run focused and full C++ verification**

Run:

```bash
cmake --build build-graph-data --target cppinsight_tests -j2
ctest --test-dir build-graph-data --output-on-failure
```

Expected: `cppinsight_tests` passes, including stable-key, qualified owner, relation and schema-version assertions.

- [x] **Step 7: Commit the scanner contract**

```bash
git add src/project/FileInfo.h src/project/ProjectScanner.h src/project/ProjectScanner.cpp src/project/ScanCache.cpp src/report/JsonReportWriter.cpp tests/ProjectScannerTests.cpp
git commit -m "feat: emit stable graph analysis facts"
```

---

### Task 2: Versioned Rust Graph Protocol

**Files:**
- Create: `desktop/src-tauri/src/graph.rs`
- Modify: `desktop/src-tauri/src/lib.rs`
- Create: `desktop/src-tauri/tests/graph_protocol.rs`

**Interfaces:**
- Produces: `pub const GRAPH_PROTOCOL_VERSION: u32 = 1`.
- Produces: `GraphQueryRequest::validate(self) -> Result<ValidatedGraphQuery, GraphQueryError>`.
- Produces: serde-tagged enums `GraphKind`, `GraphDirection`, `GraphNodeKind`, `GraphConfidence` using `snake_case` strings.
- Produces: `GraphQueryResponse` with the required spec fields and deterministic arrays.
- Consumes later: `index::query_graph`, Tauri/CLI/MCP transports, and frontend TypeScript types.

- [x] **Step 1: Write failing public serialization tests**

Create integration tests that deserialize this exact request:

```json
{
  "protocol_version": 1,
  "project_id": 7,
  "graph_kind": "function_calls",
  "root_node_ids": ["sym:0123456789abcdef"],
  "direction": "both",
  "depth": 2,
  "filters": {
    "module_ids": [],
    "file_paths": ["src/main.cpp"],
    "symbol_kinds": ["function"],
    "relation_kinds": ["calls"],
    "confidence": ["resolved", "heuristic"],
    "search": "run"
  },
  "max_nodes": 500,
  "path_target": null,
  "continuation": null
}
```

Assert the response JSON always contains `nodes`, `edges`, `groups`, `evidence`, `confidence`, `statistics`, `truncated`, `truncation_reason`, and `continuation`.

- [x] **Step 2: Run the protocol test to verify it fails**

Run:

```bash
cargo test --manifest-path desktop/src-tauri/Cargo.toml --test graph_protocol
```

Expected: compilation fails because `cppinsight_ai_lib::graph` does not exist.

- [x] **Step 3: Implement the exact request types and validation**

Use these shapes:

```rust
pub struct GraphQueryRequest {
    pub protocol_version: u32,
    pub project_id: i64,
    pub graph_kind: GraphKind,
    #[serde(default)] pub root_node_ids: Vec<String>,
    #[serde(default)] pub direction: GraphDirection,
    #[serde(default = "default_depth")] pub depth: u32,
    #[serde(default)] pub filters: GraphFilters,
    #[serde(default = "default_max_nodes")] pub max_nodes: usize,
    #[serde(default)] pub path_target: Option<String>,
    #[serde(default)] pub continuation: Option<String>,
}

pub struct GraphQueryResponse {
    pub protocol_version: u32,
    pub project_id: i64,
    pub graph_kind: GraphKind,
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub groups: Vec<GraphGroup>,
    pub evidence: Vec<GraphEvidence>,
    pub confidence: GraphConfidenceSummary,
    pub statistics: GraphStatistics,
    pub truncated: bool,
    pub truncation_reason: Option<String>,
    pub continuation: Option<String>,
}
```

`GraphNode` includes `id`, `kind`, `label`, `qualified_name`, `file`, `line`, `module_id`, `confidence`, `ambiguous`, and a string map `metadata`. `GraphEdge` includes `id`, `source`, `target`, `kind`, `weight`, `confidence`, `ambiguous`, and `evidence_ids`. `GraphStatistics` includes `original_node_count`, `returned_node_count`, `original_edge_count`, `returned_edge_count`, `cycle_count`, `unresolved_edge_count`, `ambiguous_edge_count`, `central_node_ids`, and `shortest_path_node_ids`.

Reject protocol versions other than `1`, non-positive project IDs, depth above `8`, max nodes outside `1..=500`, malformed stable IDs, a continuation token not shaped as `v1:<unsigned-offset>`, and a `path_target` absent from all requested roots only after the store has loaded candidates.

- [x] **Step 4: Run protocol tests and format**

Run:

```bash
cargo fmt --manifest-path desktop/src-tauri/Cargo.toml -- --check
cargo test --manifest-path desktop/src-tauri/Cargo.toml --test graph_protocol
```

Expected: protocol serialization and validation tests pass.

- [x] **Step 5: Commit the graph contract**

```bash
git add desktop/src-tauri/src/graph.rs desktop/src-tauri/src/lib.rs desktop/src-tauri/tests/graph_protocol.rs
git commit -m "feat: define versioned graph query protocol"
```

---

### Task 3: SQLite Graph Migration and Complete Generations

**Files:**
- Create: `desktop/src-tauri/src/index.rs`
- Modify: `desktop/src-tauri/src/lib.rs`
- Modify: `desktop/src-tauri/tests/graph_protocol.rs`

**Interfaces:**
- Produces: `index::migrate_database(connection: &Connection) -> Result<(), String>`.
- Produces: `index::write_index(connection: &mut Connection, scan: &ScanResult) -> Result<i64, String>`.
- Produces: `index::stable_symbol_key(symbol: &SymbolInfo) -> String` as a schema-2 fallback matching the C++ FNV-1a algorithm.
- Produces: `index::load_index_status(connection, project_id) -> Result<IndexStatus, String>`.
- Preserves: existing `open_database_at`, `write_index`, `load_project_overview`, legacy-database migration, Ask and inspect public entry points through `lib.rs` re-exports.

- [x] **Step 1: Write failing migration and rollback tests**

Cover all of these cases:

```rust
assert_eq!(column_text(&db, "symbols", "stable_key"), "TEXT");
assert!(table_exists(&db, "symbol_relations"));
assert!(table_exists(&db, "module_relations"));
assert!(table_exists(&db, "graph_layouts"));
assert_eq!(load_index_status(&db, project_id)?.schema_version, 3);
assert!(load_index_status(&db, project_id)?.complete);
```

Index a schema-2 fixture with empty stable keys and assert a deterministic `sym:` key is generated. Add an injected `WriteFault::BeforeCompleteMarker` test-only hook, attempt a second index, assert it returns an error, then assert the old generation marker and old symbol rows are unchanged.

- [x] **Step 2: Run migration tests to verify they fail**

Run:

```bash
cargo test --manifest-path desktop/src-tauri/Cargo.toml --test graph_protocol migration
```

Expected: the graph metadata columns and tables are missing.

- [x] **Step 3: Move schema/write ownership into `index.rs`**

Create or migrate these columns with idempotent `ALTER TABLE` helpers:

```sql
ALTER TABLE projects ADD COLUMN index_schema_version INTEGER NOT NULL DEFAULT 2;
ALTER TABLE projects ADD COLUMN generator_version TEXT NOT NULL DEFAULT 'legacy';
ALTER TABLE projects ADD COLUMN index_generation INTEGER NOT NULL DEFAULT 0;
ALTER TABLE projects ADD COLUMN index_complete INTEGER NOT NULL DEFAULT 1;
ALTER TABLE symbols ADD COLUMN stable_key TEXT NOT NULL DEFAULT '';
ALTER TABLE symbols ADD COLUMN qualified_name TEXT NOT NULL DEFAULT '';
ALTER TABLE symbols ADD COLUMN parent_stable_key TEXT NOT NULL DEFAULT '';
ALTER TABLE symbols ADD COLUMN parent_qualified_name TEXT NOT NULL DEFAULT '';
ALTER TABLE calls ADD COLUMN caller_stable_key TEXT NOT NULL DEFAULT '';
ALTER TABLE calls ADD COLUMN caller_qualified_name TEXT NOT NULL DEFAULT '';
ALTER TABLE calls ADD COLUMN target_qualified_name TEXT NOT NULL DEFAULT '';
ALTER TABLE calls ADD COLUMN resolved_target_key TEXT NOT NULL DEFAULT '';
ALTER TABLE calls ADD COLUMN candidate_target_keys_json TEXT NOT NULL DEFAULT '[]';
```

Create:

```sql
CREATE TABLE IF NOT EXISTS symbol_relations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    project_id INTEGER NOT NULL,
    source_key TEXT NOT NULL,
    target_key TEXT NOT NULL DEFAULT '',
    target_name TEXT NOT NULL,
    relation_kind TEXT NOT NULL,
    evidence_file TEXT NOT NULL,
    evidence_line INTEGER NOT NULL,
    confidence TEXT NOT NULL,
    ambiguous INTEGER NOT NULL DEFAULT 0,
    candidate_target_keys_json TEXT NOT NULL DEFAULT '[]',
    FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE
);
CREATE TABLE IF NOT EXISTS module_relations (
    project_id INTEGER NOT NULL,
    source_module TEXT NOT NULL,
    target_module TEXT NOT NULL,
    relation_kind TEXT NOT NULL,
    weight INTEGER NOT NULL,
    confidence TEXT NOT NULL,
    evidence_count INTEGER NOT NULL,
    PRIMARY KEY(project_id, source_module, target_module, relation_kind),
    FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE
);
CREATE TABLE IF NOT EXISTS graph_layouts (
    project_id INTEGER NOT NULL,
    graph_kind TEXT NOT NULL,
    view_key TEXT NOT NULL,
    protocol_version INTEGER NOT NULL,
    positions_json TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY(project_id, graph_kind, view_key),
    FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE
);
```

Add indexes on `(project_id, stable_key)`, `(project_id, qualified_name)`, relation source/target/kind, and caller/resolved target.

- [x] **Step 4: Preserve the previous complete generation on failure**

Within one `rusqlite::Transaction`, compute `next_generation = current + 1`, replace facts/relations, resolve them, then set:

```sql
UPDATE projects
SET index_schema_version = 3,
    generator_version = ?2,
    index_generation = ?3,
    index_complete = 1
WHERE id = ?1;
```

Commit only after this update. The test-only fault returns before the marker, causing transaction rollback. Keep `index_complete = 1` for every externally visible committed generation because incomplete transaction state is never committed.

- [x] **Step 5: Update legacy copy and compatibility re-exports**

When copying old database rows, populate new columns with stable-key fallbacks and immediately run relation normalization in the destination transaction. Existing callers continue importing `crate::write_index` and `crate::open_database_at`; `lib.rs` re-exports the moved functions.

- [x] **Step 6: Run migration, rollback and full Rust tests**

Run:

```bash
cargo fmt --manifest-path desktop/src-tauri/Cargo.toml -- --check
cargo test --manifest-path desktop/src-tauri/Cargo.toml
```

Expected: existing tests and new migration/rollback tests all pass.

- [x] **Step 7: Commit the generation-safe graph store**

```bash
git add desktop/src-tauri/src/index.rs desktop/src-tauri/src/lib.rs desktop/src-tauri/tests/graph_protocol.rs
git commit -m "feat: migrate index to generation-safe graph schema"
```

---

### Task 4: Relation Resolution and Module Aggregation

**Files:**
- Modify: `desktop/src-tauri/src/index.rs`
- Modify: `desktop/src-tauri/tests/graph_protocol.rs`

**Interfaces:**
- Produces: `resolve_graph_facts(transaction: &Transaction, project_id: i64) -> Result<ResolutionSummary, String>`.
- Produces: candidate resolution ranked by exact stable key, exact qualified name, same owner scope, same module, then unqualified name.
- Produces: `calls.resolved_target_key` only when exactly one highest-ranked target exists; otherwise preserves sorted unique candidates and sets the relation `ambiguous = 1`.
- Produces: file nodes `file:<normalized-project-relative-path>` and module nodes `module:<top-level-or-dot>` without storing duplicate node tables.
- Produces: weighted `module_relations` from resolved includes, cross-file calls, and resolved class facts.

- [x] **Step 1: Write failing exact, ambiguous, unresolved and aggregation tests**

Use an in-memory project containing `app::run`, `util::run`, `app::main`, resolved/unresolved includes, inheritance and calls. Assert:

```rust
assert_eq!(resolved_call.resolved_target_key, app_run_key);
assert_eq!(ambiguous_call.resolved_target_key, "");
assert_eq!(ambiguous_call.candidate_target_keys, vec![app_run_key, util_run_key]);
assert_eq!(unresolved_relation.confidence, GraphConfidence::Unresolved);
assert_eq!(module_edge.weight, 3);
assert_eq!(module_edge.evidence_count, 3);
```

Assert repeated indexing produces byte-for-byte identical ordered candidate JSON.

- [x] **Step 2: Run relation tests to verify they fail**

Run:

```bash
cargo test --manifest-path desktop/src-tauri/Cargo.toml --test graph_protocol relation_resolution
```

Expected: rows are present but targets and module weights are not resolved.

- [x] **Step 3: Implement deterministic candidate resolution**

Load definitions into maps keyed by stable key, qualified name, and unqualified name. Score candidates with exact integer tiers `500`, `400`, `300`, `200`, and `100`; retain all candidates tied at the best tier, sorted by stable key. A single candidate yields `resolved`; multiple candidates yield `ambiguous`; no candidate yields `unresolved`. Never select by SQLite insertion order.

- [x] **Step 4: Materialize relation and module rows**

Create one `symbol_relations` row per call or C++ type-relation fact. For unresolved targets, keep `target_key = ''` and the raw `target_name`. Aggregate each cross-module relation with:

```sql
INSERT INTO module_relations(
    project_id, source_module, target_module, relation_kind,
    weight, confidence, evidence_count
) VALUES(?1, ?2, ?3, ?4, 1, ?5, 1)
ON CONFLICT(project_id, source_module, target_module, relation_kind)
DO UPDATE SET
    weight = weight + 1,
    evidence_count = evidence_count + 1,
    confidence = CASE
        WHEN excluded.confidence = 'unresolved' THEN 'unresolved'
        WHEN excluded.confidence = 'ambiguous' AND confidence != 'unresolved' THEN 'ambiguous'
        ELSE confidence
    END;
```

- [x] **Step 5: Run focused and full Rust tests**

Run:

```bash
cargo test --manifest-path desktop/src-tauri/Cargo.toml --test graph_protocol relation_resolution
cargo test --manifest-path desktop/src-tauri/Cargo.toml
```

Expected: resolution, ambiguity, unresolved and aggregation tests pass without changing legacy inspect behavior.

- [x] **Step 6: Commit relation normalization**

```bash
git add desktop/src-tauri/src/index.rs desktop/src-tauri/tests/graph_protocol.rs
git commit -m "feat: resolve graph relations and module weights"
```

---

### Task 5: Bounded Subgraphs, Paths, Cycles and Key Nodes

**Files:**
- Modify: `desktop/src-tauri/src/graph.rs`
- Modify: `desktop/src-tauri/src/index.rs`
- Modify: `desktop/src-tauri/tests/graph_protocol.rs`

**Interfaces:**
- Produces: `pub fn query_graph(connection: &Connection, request: GraphQueryRequest) -> Result<GraphQueryResponse, GraphQueryError>`.
- Produces all five `GraphKind` values: `module_dependencies`, `function_calls`, `class_relations`, `file_includes`, `entry_paths`.
- Produces deterministic BFS by requested direction/depth, filters before paging, shortest path when `path_target` is present, cycle membership, degree-central nodes, and `v1:<offset>` continuation.
- Consumes: normalized facts and relations from Tasks 3–4.

- [x] **Step 1: Write one failing behavioral test per graph kind**

Assert:

```rust
let modules = query(GraphKind::ModuleDependencies, vec![], GraphDirection::Both, 1);
assert!(edge_kinds(&modules).contains("include"));

let calls = query(GraphKind::FunctionCalls, vec![main_key], GraphDirection::Outgoing, 2);
assert_eq!(node_ids(&calls), sorted_unique(node_ids(&calls)));

let classes = query(GraphKind::ClassRelations, vec![runner_key], GraphDirection::Both, 2);
assert!(edge_kinds(&classes).contains("inherits"));

let includes = query(GraphKind::FileIncludes, vec![file_main], GraphDirection::Outgoing, 2);
assert!(node_ids(&includes).iter().all(|id| id.starts_with("file:")));

let entries = query_path(GraphKind::EntryPaths, vec![entry_key], target_key);
assert_eq!(entries.statistics.shortest_path_node_ids.first(), Some(&entry_key));
assert_eq!(entries.statistics.shortest_path_node_ids.last(), Some(&target_key));
```

Add filter/depth tests, a three-node cycle, a disconnected path, ambiguity evidence, and a 620-node synthetic chain requested with `max_nodes = 500` then continued with the returned token.

- [x] **Step 2: Run query tests to verify they fail**

Run:

```bash
cargo test --manifest-path desktop/src-tauri/Cargo.toml --test graph_protocol graph_query
```

Expected: `query_graph` is missing or returns empty graphs.

- [x] **Step 3: Load canonical nodes and edges for each graph kind**

Use only these mappings:

```text
module_dependencies -> module_relations; node IDs already begin module:
function_calls      -> symbol_relations where relation_kind = calls
class_relations     -> symbol_relations where relation_kind in inherits,implements,contains,uses
file_includes       -> includes; node IDs are file:<normalized path>
entry_paths         -> calls edges, roots default to entry-point symbol keys
```

Create evidence IDs as `ev:` + FNV-1a of edge kind, source, target, file, and line. Include raw target names as unresolved placeholder nodes only when `confidence` filters allow `unresolved`; placeholder IDs start `unresolved:` plus a stable hash.

- [x] **Step 4: Implement deterministic traversal and filters**

Build sorted adjacency lists and perform BFS from roots in `incoming`, `outgoing`, or `both` direction up to depth `8`. With no roots, return the graph kind's top degree nodes and their one-hop neighborhood. Apply module, file, symbol-kind, relation-kind, confidence and case-folded search filters before computing original counts and paging.

- [x] **Step 5: Implement analysis and continuation**

Use BFS for shortest path, Tarjan strongly connected components for cycles, and `(in_degree + out_degree, stable_id)` ordering for central nodes. The first page includes nodes `[0..max_nodes)`; `v1:<offset>` requests subsequent deterministic node slices. Include only edges whose endpoints occur in that page, and report full filtered counts as `original_*`. Set:

```rust
truncated = original_node_count > offset + returned_node_count;
truncation_reason = truncated.then(|| "node_limit".to_string());
continuation = truncated.then(|| format!("v1:{}", offset + returned_node_count));
```

- [x] **Step 6: Run focused, full and lint verification**

Run:

```bash
cargo fmt --manifest-path desktop/src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path desktop/src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path desktop/src-tauri/Cargo.toml
```

Expected: five graph-kind tests, analytics, filters, depth, truncation and all legacy tests pass.

- [x] **Step 7: Commit the graph query engine**

```bash
git add desktop/src-tauri/src/graph.rs desktop/src-tauri/src/index.rs desktop/src-tauri/tests/graph_protocol.rs
git commit -m "feat: query bounded code relationship graphs"
```

---

### Task 6: Saved Layouts and Desktop Graph API

**Files:**
- Modify: `desktop/src-tauri/src/index.rs`
- Modify: `desktop/src-tauri/src/commands.rs`
- Modify: `desktop/src-tauri/src/lib.rs`
- Modify: `desktop/src/types.ts`
- Modify: `desktop/src/api.ts`
- Modify: `desktop/src-tauri/tests/graph_protocol.rs`

**Interfaces:**
- Produces: `SavedGraphLayout { protocol_version, project_id, graph_kind, view_key, positions, updated_at }`.
- Produces: `query_graph(root, request)`, `save_graph_layout(root, layout)`, and `load_graph_layout(root, project_id, graph_kind, view_key)` Tauri commands.
- Produces: TypeScript types that exactly mirror Rust serialized names.

- [x] **Step 1: Write failing layout round-trip and replacement tests**

Save positions for two stable nodes, load them, replace one view key, and assert unrelated graph/view keys remain unchanged. Reject a layout with protocol version other than `1`, an empty view key, more than `500` positions, non-finite coordinates, or node IDs not matching a stable graph-node prefix.

- [x] **Step 2: Run layout tests to verify they fail**

Run:

```bash
cargo test --manifest-path desktop/src-tauri/Cargo.toml --test graph_protocol graph_layout
```

Expected: saved-layout functions are missing.

- [x] **Step 3: Implement store methods and non-blocking commands**

Use `INSERT ... ON CONFLICT ... DO UPDATE` for layout replacement. Run graph queries and layout SQLite access through `tauri::async_runtime::spawn_blocking`. Register all three commands in `generate_handler!`.

- [x] **Step 4: Add exact TypeScript protocol and API wrappers**

Add discriminated string unions and interfaces matching Rust, then expose:

```ts
export function queryGraph(root: string, request: GraphQueryRequest): Promise<GraphQueryResponse> {
  return invoke<GraphQueryResponse>("query_graph", { root, request });
}

export function saveGraphLayout(root: string, layout: SavedGraphLayout): Promise<void> {
  return invoke<void>("save_graph_layout", { root, layout });
}

export function loadGraphLayout(
  root: string,
  projectId: number,
  graphKind: GraphKind,
  viewKey: string
): Promise<SavedGraphLayout | null> {
  return invoke<SavedGraphLayout | null>("load_graph_layout", {
    root, projectId, graphKind, viewKey
  });
}
```

- [x] **Step 5: Run Rust and TypeScript checks**

Run:

```bash
cargo test --manifest-path desktop/src-tauri/Cargo.toml
pnpm --dir desktop run build
```

Expected: layout tests, Rust suite, TypeScript compiler and Vite build pass.

- [x] **Step 6: Commit the desktop graph boundary**

```bash
git add desktop/src-tauri/src/index.rs desktop/src-tauri/src/commands.rs desktop/src-tauri/src/lib.rs desktop/src/types.ts desktop/src/api.ts desktop/src-tauri/tests/graph_protocol.rs
git commit -m "feat: expose graph queries and saved layouts to desktop"
```

---

### Task 7: Shared CLI and MCP Graph Transport

**Files:**
- Modify: `desktop/src-tauri/src/bin/workbench.rs`
- Modify: `tools/mcp-server/index.mjs`
- Modify: `tools/mcp-server/package.json`
- Create: `tools/mcp-server/test/index.test.mjs`

**Interfaces:**
- Produces CLI: `cppinsight-workbench graph --root <project-root> --request <request.json|-> [--db <sqlite>]`.
- CLI behavior: resolves the project ID from `--root`, overwrites `request.project_id` with the resolved local ID, runs `query_graph`, and writes only pretty JSON to stdout.
- Produces MCP tool: `cppinsight_graph` with `projectPath` and the same request properties excluding `project_id`.
- Consumes: `GraphQueryRequest` and `query_graph` from Task 5.

- [x] **Step 1: Write failing CLI parser and MCP forwarding tests**

Refactor CLI argument execution into a library-testable `run_with_io(args, stdin, stdout)` function. Assert `--request -` reads JSON from stdin, rejects wrong protocol versions, and returns parseable response JSON. In Node tests, stub the child runner and assert `cppinsight_graph` forwards `graph --root <path> --request -` with the request on stdin, clamps no values silently, and propagates structured validation errors.

- [x] **Step 2: Run transport tests to verify they fail**

Run:

```bash
cargo test --manifest-path desktop/src-tauri/Cargo.toml --bin cppinsight-workbench graph
pnpm --dir tools/mcp-server test
```

Expected: graph command and MCP test script do not exist.

- [x] **Step 3: Implement CLI JSON transport**

Add usage text and parse exactly one `--request` value. Read a file or stdin, deserialize `GraphQueryRequest`, resolve the database/project, replace `project_id`, execute the shared query, and output `serde_json::to_writer_pretty` plus one newline. Do not reproduce graph traversal in the CLI.

- [x] **Step 4: Implement MCP schema and stdin forwarding**

Add `cppinsight_graph` with enums for graph kind/direction and a maximum `maxNodes` of `500`. Convert camelCase MCP input to snake_case protocol JSON and invoke the CLI through stdin so project paths and filter content are not placed into one long command argument. Keep the existing inspect and Ask tools unchanged.

- [ ] **Step 5: Run Rust and MCP verification**

Run:

```bash
cargo test --manifest-path desktop/src-tauri/Cargo.toml
pnpm --dir tools/mcp-server test
```

Expected: graph CLI/MCP tests and all existing tests pass.

- [ ] **Step 6: Commit transport reuse**

```bash
git add desktop/src-tauri/src/bin/workbench.rs tools/mcp-server/index.mjs tools/mcp-server/package.json tools/mcp-server/test/index.test.mjs
git commit -m "feat: expose graph protocol through CLI and MCP"
```

---

### Task 8: Protocol Documentation and Phase Verification

**Files:**
- Create: `docs/graph-protocol-v1.md`
- Modify: `desktop/README.md`
- Modify: `tools/mcp-server/README.md`
- Modify: `docs/superpowers/plans/2026-08-28-graph-data-foundation.md`

**Interfaces:**
- Documents the exact version-1 request/response, stable IDs, relation confidence, ambiguity, continuation, layout storage, CLI examples and compatibility guarantees.
- Produces verification evidence for the next Graph Workbench phase.

- [ ] **Step 1: Write the protocol document from executable examples**

Include one complete request and response for each graph kind, the exact validation limits, schema-2 fallback behavior, schema-3 scan additions, confidence meanings, unresolved placeholder behavior, and continuation semantics. Use only examples copied from passing tests or generated by the CLI fixture.

- [ ] **Step 2: Update CLI and MCP readmes**

Document:

```bash
cppinsight-workbench graph --root D:\repos\demo --request graph-request.json
cppinsight-workbench graph --root D:\repos\demo --request - < graph-request.json
```

Explain that desktop, CLI and MCP share protocol version `1`, graph data stays local, and AI is not required.

- [ ] **Step 3: Run the complete phase verification matrix**

Run:

```bash
cmake --build build-graph-data --target cppinsight_tests -j2
ctest --test-dir build-graph-data --output-on-failure
cargo fmt --manifest-path desktop/src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path desktop/src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path desktop/src-tauri/Cargo.toml
pnpm --dir tools/mcp-server test
pnpm --dir desktop run build
pnpm --dir desktop run verify:release
git diff --check d170464..HEAD
```

Expected: every command exits zero; Rust tests include migration, resolution, five graph kinds, analytics, filtering, depth, truncation, continuation, rollback, layouts and transport.

- [ ] **Step 4: Self-review plan coverage and update checked boxes**

Confirm the phase covers stable IDs, qualified names/parents, call ownership/candidates, class relations, entry points, evidence/confidence, migrations, complete generations, module relations, saved layouts, all five query kinds, direction/depth/filtering, path/cycles/key nodes, aggregation, truncation/continuation, and desktop/CLI/MCP reuse. Leave Cytoscape rendering, report integration and benchmark corpus unchecked because they are explicit later phases.

- [ ] **Step 5: Commit documentation and verification record**

```bash
git add docs/graph-protocol-v1.md desktop/README.md tools/mcp-server/README.md docs/superpowers/plans/2026-08-28-graph-data-foundation.md
git commit -m "docs: publish graph protocol version one"
```

## Phase Acceptance

This plan is complete only when:

1. Unchanged entities keep the same stable IDs across rescans and harmless line shifts.
2. Schema-2 JSON and databases upgrade safely; a failed write preserves the prior complete generation.
3. Calls and type relations retain source owner, evidence, confidence, resolved target or ordered ambiguity candidates.
4. Module weights and all five graph kinds come from the normalized index.
5. Queries honor direction, depth, filters, shortest paths, cycles, key nodes and the 500-node limit.
6. Truncation reports original counts and a usable deterministic continuation token.
7. Desktop, CLI and MCP serialize the same protocol-version-1 model.
8. Saved graph layouts round-trip by stable node ID and scoped view key.
9. C++, Rust, MCP, TypeScript, release-verifier and diff checks all pass.
