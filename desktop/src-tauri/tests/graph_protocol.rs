use cppinsight_ai_lib::graph::{
    query_graph, GraphDirection, GraphFilters, GraphKind, GraphQueryRequest, GraphQueryResponse,
    GRAPH_PROTOCOL_VERSION,
};
use cppinsight_ai_lib::index::{load_index_status, INDEX_SCHEMA_VERSION};
use cppinsight_ai_lib::{
    open_database_at, write_index, CallInfo, EntryPointInfo, IncludeInfo, LineStats, ScanResult,
    ScanStats, SymbolInfo, SymbolRelationInfo,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn valid_request_json() -> Value {
    json!({
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
    })
}

fn schema_two_scan(root: &str, symbol_name: &str) -> ScanResult {
    ScanResult {
        schema_version: 2,
        generator_version: String::new(),
        root: root.to_string(),
        mode: "heuristic".to_string(),
        confidence: "heuristic".to_string(),
        scan_duration_ms: 1,
        compile_commands: String::new(),
        compile_commands_entries: 0,
        reused_file_count: 0,
        file_count: 0,
        skipped_entries: 0,
        stats: ScanStats {
            source_files: 0,
            header_files: 0,
            inline_files: 0,
            total_bytes: 0,
            lines: LineStats::default(),
        },
        extensions: BTreeMap::new(),
        top_level_layout: BTreeMap::new(),
        second_level_layout: BTreeMap::new(),
        diagnostics: Vec::new(),
        includes: Vec::new(),
        symbols: vec![SymbolInfo {
            file: "src/main.cpp".to_string(),
            line: 10,
            kind: "function".to_string(),
            name: symbol_name.to_string(),
            qualified_name: String::new(),
            stable_key: String::new(),
            parent_qualified_name: String::new(),
            parent_stable_key: String::new(),
            signature: format!("void {symbol_name}()"),
            scope: String::new(),
            is_definition: true,
            confidence: "heuristic".to_string(),
        }],
        calls: vec![CallInfo {
            file: "src/main.cpp".to_string(),
            line: 20,
            name: symbol_name.to_string(),
            target_qualified_name: String::new(),
            caller_qualified_name: String::new(),
            caller_stable_key: String::new(),
            confidence: "heuristic".to_string(),
        }],
        relations: Vec::new(),
        entry_points: vec![EntryPointInfo {
            file: "app/main.cpp".to_string(),
            line: 1,
            qualified_name: "app::main".to_string(),
            symbol_stable_key: "sym:0000000000000001".to_string(),
            kind: "program".to_string(),
            confidence: "heuristic".to_string(),
        }],
        files: Vec::new(),
    }
}

fn graph_request(project_id: i64, graph_kind: GraphKind, roots: &[&str]) -> GraphQueryRequest {
    GraphQueryRequest {
        protocol_version: GRAPH_PROTOCOL_VERSION,
        project_id,
        graph_kind,
        root_node_ids: roots.iter().map(|root| (*root).to_string()).collect(),
        direction: GraphDirection::Both,
        depth: 2,
        filters: GraphFilters::default(),
        max_nodes: 500,
        path_target: None,
        continuation: None,
    }
}

fn graph_resolution_scan(root: &str) -> ScanResult {
    let symbol =
        |key: &str, qualified_name: &str, name: &str, kind: &str, file: &str, line: u64| {
            SymbolInfo {
                file: file.to_string(),
                line,
                kind: kind.to_string(),
                name: name.to_string(),
                qualified_name: qualified_name.to_string(),
                stable_key: key.to_string(),
                parent_qualified_name: qualified_name
                    .rsplit_once("::")
                    .map(|(parent, _)| parent.to_string())
                    .unwrap_or_default(),
                parent_stable_key: String::new(),
                signature: format!("{kind} {qualified_name}"),
                scope: qualified_name
                    .rsplit_once("::")
                    .map(|(parent, _)| parent.to_string())
                    .unwrap_or_default(),
                is_definition: true,
                confidence: "heuristic".to_string(),
            }
        };
    let call = |line: u64,
                caller_key: &str,
                caller_name: &str,
                name: &str,
                target_qualified_name: &str,
                file: &str| CallInfo {
        file: file.to_string(),
        line,
        name: name.to_string(),
        target_qualified_name: target_qualified_name.to_string(),
        caller_qualified_name: caller_name.to_string(),
        caller_stable_key: caller_key.to_string(),
        confidence: "heuristic".to_string(),
    };

    ScanResult {
        schema_version: 3,
        generator_version: "cppinsight-core/0.1.0".to_string(),
        root: root.to_string(),
        mode: "heuristic".to_string(),
        confidence: "heuristic".to_string(),
        scan_duration_ms: 1,
        compile_commands: String::new(),
        compile_commands_entries: 0,
        reused_file_count: 0,
        file_count: 0,
        skipped_entries: 0,
        stats: ScanStats::default(),
        extensions: BTreeMap::new(),
        top_level_layout: BTreeMap::new(),
        second_level_layout: BTreeMap::new(),
        diagnostics: Vec::new(),
        includes: ["one.h", "two.h", "three.h"]
            .into_iter()
            .enumerate()
            .map(|(index, target)| IncludeInfo {
                file: "app/main.cpp".to_string(),
                line: (index + 1) as u64,
                target: target.to_string(),
                kind: "local".to_string(),
                resolved: true,
                resolved_path: format!("util/{target}"),
            })
            .collect(),
        symbols: vec![
            symbol(
                "sym:0000000000000001",
                "app::main",
                "main",
                "function",
                "app/main.cpp",
                1,
            ),
            symbol(
                "sym:0000000000000002",
                "main",
                "main",
                "function",
                "main.cpp",
                1,
            ),
            symbol(
                "sym:0000000000000003",
                "app::run",
                "run",
                "function",
                "app/run.cpp",
                5,
            ),
            symbol(
                "sym:0000000000000004",
                "util::run",
                "run",
                "function",
                "util/run.cpp",
                5,
            ),
            symbol(
                "sym:0000000000000005",
                "app::Runner",
                "Runner",
                "class",
                "app/runner.h",
                3,
            ),
            symbol(
                "sym:0000000000000006",
                "app::Base",
                "Base",
                "class",
                "app/base.h",
                3,
            ),
        ],
        calls: vec![
            call(
                10,
                "sym:0000000000000001",
                "app::main",
                "run",
                "app::run",
                "app/main.cpp",
            ),
            call(20, "sym:0000000000000002", "main", "run", "", "main.cpp"),
            call(
                30,
                "sym:0000000000000001",
                "app::main",
                "missing",
                "app::missing",
                "app/main.cpp",
            ),
        ],
        relations: vec![
            SymbolRelationInfo {
                file: "app/runner.h".to_string(),
                line: 3,
                source_qualified_name: "app::Runner".to_string(),
                source_stable_key: "sym:0000000000000005".to_string(),
                target_name: "app::Base".to_string(),
                kind: "inherits".to_string(),
                confidence: "heuristic".to_string(),
            },
            SymbolRelationInfo {
                file: "app/runner.h".to_string(),
                line: 4,
                source_qualified_name: "app::Runner".to_string(),
                source_stable_key: "sym:0000000000000005".to_string(),
                target_name: "app::MissingType".to_string(),
                kind: "uses".to_string(),
                confidence: "heuristic".to_string(),
            },
        ],
        entry_points: vec![EntryPointInfo {
            file: "app/main.cpp".to_string(),
            line: 1,
            qualified_name: "app::main".to_string(),
            symbol_stable_key: "sym:0000000000000001".to_string(),
            kind: "program".to_string(),
            confidence: "heuristic".to_string(),
        }],
        files: Vec::new(),
    }
}

fn table_exists(connection: &rusqlite::Connection, table: &str) -> bool {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
            [table],
            |row| row.get::<_, i64>(0),
        )
        .unwrap()
        == 1
}

fn column_exists(connection: &rusqlite::Connection, table: &str, column: &str) -> bool {
    let mut statement = connection
        .prepare(&format!("PRAGMA table_info({table})"))
        .unwrap();
    let found = statement
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .any(|name| name.unwrap() == column);
    found
}

#[test]
fn protocol_v1_deserializes_the_shared_request_shape() {
    let request: GraphQueryRequest = serde_json::from_value(valid_request_json()).unwrap();

    assert_eq!(request.protocol_version, GRAPH_PROTOCOL_VERSION);
    assert_eq!(request.project_id, 7);
    assert_eq!(request.graph_kind, GraphKind::FunctionCalls);
    assert_eq!(request.direction, GraphDirection::Both);
    assert_eq!(request.root_node_ids, ["sym:0123456789abcdef"]);
    assert_eq!(request.filters.file_paths, ["src/main.cpp"]);
    assert_eq!(request.filters.search.as_deref(), Some("run"));
    assert_eq!(request.validate().unwrap().continuation_offset, 0);
}

#[test]
fn protocol_v1_empty_response_keeps_every_required_top_level_field() {
    let response = GraphQueryResponse::empty(7, GraphKind::FunctionCalls);
    let value = serde_json::to_value(response).unwrap();

    for field in [
        "nodes",
        "edges",
        "groups",
        "evidence",
        "confidence",
        "statistics",
        "truncated",
        "truncation_reason",
        "continuation",
    ] {
        assert!(
            value.get(field).is_some(),
            "missing required field: {field}"
        );
    }
    assert_eq!(value["protocol_version"], 1);
    assert_eq!(value["project_id"], 7);
    assert_eq!(value["graph_kind"], "function_calls");
}

#[test]
fn protocol_v1_rejects_invalid_versions_limits_ids_and_continuations() {
    let cases = [
        (
            "protocol_version",
            json!(2),
            "unsupported graph protocol version",
        ),
        ("project_id", json!(0), "project_id must be positive"),
        ("depth", json!(9), "depth must be at most 8"),
        ("max_nodes", json!(0), "max_nodes must be between 1 and 500"),
        (
            "max_nodes",
            json!(501),
            "max_nodes must be between 1 and 500",
        ),
        (
            "root_node_ids",
            json!(["symbol-without-prefix"]),
            "invalid graph node id",
        ),
        ("continuation", json!("v2:12"), "invalid continuation token"),
        (
            "continuation",
            json!("v1:not-a-number"),
            "invalid continuation token",
        ),
    ];

    for (field, invalid_value, expected_error) in cases {
        let mut value = valid_request_json();
        value[field] = invalid_value;
        let request: GraphQueryRequest = serde_json::from_value(value).unwrap();
        let error = request.validate().unwrap_err().to_string();
        assert!(
            error.contains(expected_error),
            "{field} produced unexpected error: {error}"
        );
    }
}

#[test]
fn protocol_v1_defaults_optional_query_controls() {
    let request: GraphQueryRequest = serde_json::from_value(json!({
        "protocol_version": 1,
        "project_id": 3,
        "graph_kind": "module_dependencies"
    }))
    .unwrap();

    assert!(request.root_node_ids.is_empty());
    assert_eq!(request.direction, GraphDirection::Both);
    assert_eq!(request.depth, 2);
    assert_eq!(request.max_nodes, 500);
    assert!(request.filters.relation_kinds.is_empty());
    assert!(request.path_target.is_none());
    assert!(request.continuation.is_none());
}

#[test]
fn migration_creates_graph_schema_and_backfills_schema_two_stable_keys() {
    let temporary = tempfile::tempdir().unwrap();
    let database = temporary.path().join("index.sqlite");
    let mut connection = open_database_at(&database).unwrap();

    for (table, column) in [
        ("projects", "index_schema_version"),
        ("projects", "generator_version"),
        ("projects", "index_generation"),
        ("projects", "index_complete"),
        ("symbols", "stable_key"),
        ("symbols", "qualified_name"),
        ("symbols", "parent_stable_key"),
        ("calls", "caller_stable_key"),
        ("calls", "resolved_target_key"),
        ("calls", "candidate_target_keys_json"),
    ] {
        assert!(
            column_exists(&connection, table, column),
            "missing {table}.{column}"
        );
    }
    for table in ["symbol_relations", "module_relations", "graph_layouts"] {
        assert!(table_exists(&connection, table), "missing table: {table}");
    }

    let project_id = write_index(&mut connection, &schema_two_scan("D:/legacy", "run")).unwrap();
    let stable_key: String = connection
        .query_row(
            "SELECT stable_key FROM symbols WHERE project_id = ?1 AND name = 'run'",
            [project_id],
            |row| row.get(0),
        )
        .unwrap();
    assert!(stable_key.starts_with("sym:"));
    assert_eq!(stable_key.len(), 20);

    let status = load_index_status(&connection, project_id).unwrap();
    assert_eq!(status.schema_version, INDEX_SCHEMA_VERSION);
    assert_eq!(status.generator_version, "legacy-json/2");
    assert_eq!(status.generation, 1);
    assert!(status.complete);
}

#[test]
fn failed_complete_generation_switch_preserves_previous_index() {
    let temporary = tempfile::tempdir().unwrap();
    let database = temporary.path().join("index.sqlite");
    let mut connection = open_database_at(&database).unwrap();
    let project_id =
        write_index(&mut connection, &schema_two_scan("D:/rollback", "before")).unwrap();
    let before = load_index_status(&connection, project_id).unwrap();

    connection
        .execute_batch(
            "CREATE TRIGGER fail_generation_switch
             BEFORE UPDATE OF index_generation ON projects
             BEGIN
               SELECT RAISE(ABORT, 'injected generation failure');
             END;",
        )
        .unwrap();

    let error = write_index(&mut connection, &schema_two_scan("D:/rollback", "after")).unwrap_err();
    assert!(error.contains("injected generation failure"));

    let after = load_index_status(&connection, project_id).unwrap();
    assert_eq!(after, before);
    let names: Vec<String> = connection
        .prepare("SELECT name FROM symbols WHERE project_id = ?1 ORDER BY name")
        .unwrap()
        .query_map([project_id], |row| row.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(names, ["before"]);
}

#[test]
fn partially_migrated_legacy_database_gets_every_missing_base_column() {
    let temporary = tempfile::tempdir().unwrap();
    let database = temporary.path().join("partial.sqlite");
    let raw = rusqlite::Connection::open(&database).unwrap();
    raw.execute_batch(
        "CREATE TABLE projects (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            root TEXT NOT NULL UNIQUE,
            indexed_at TEXT NOT NULL,
            file_count INTEGER NOT NULL,
            symbol_count INTEGER NOT NULL,
            include_count INTEGER NOT NULL,
            diagnostics_json TEXT NOT NULL,
            mode TEXT NOT NULL DEFAULT 'heuristic'
         );
         CREATE TABLE files (
            project_id INTEGER NOT NULL, path TEXT NOT NULL, extension TEXT NOT NULL,
            category TEXT NOT NULL, size_bytes INTEGER NOT NULL, total_lines INTEGER NOT NULL,
            code_lines INTEGER NOT NULL, comment_lines INTEGER NOT NULL,
            blank_lines INTEGER NOT NULL, PRIMARY KEY(project_id, path)
         );
         CREATE TABLE symbols (
            id INTEGER PRIMARY KEY AUTOINCREMENT, project_id INTEGER NOT NULL,
            file TEXT NOT NULL, line INTEGER NOT NULL, kind TEXT NOT NULL,
            name TEXT NOT NULL, signature TEXT NOT NULL, scope TEXT NOT NULL,
            is_definition INTEGER NOT NULL
         );
         CREATE TABLE includes (
            id INTEGER PRIMARY KEY AUTOINCREMENT, project_id INTEGER NOT NULL,
            file TEXT NOT NULL, line INTEGER NOT NULL, target TEXT NOT NULL,
            kind TEXT NOT NULL
         );
         CREATE TABLE chunks (
            id INTEGER PRIMARY KEY AUTOINCREMENT, project_id INTEGER NOT NULL,
            file TEXT NOT NULL, start_line INTEGER NOT NULL, end_line INTEGER NOT NULL,
            text TEXT NOT NULL
         );
         CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
    )
    .unwrap();
    drop(raw);

    let migrated = open_database_at(&database).unwrap();
    for (table, column) in [
        ("projects", "confidence"),
        ("files", "mtime_unix"),
        ("symbols", "confidence"),
        ("includes", "resolved"),
        ("includes", "resolved_path"),
        ("calls", "confidence"),
    ] {
        assert!(
            column_exists(&migrated, table, column),
            "missing {table}.{column}"
        );
    }
}

#[test]
fn relation_resolution_is_deterministic_and_preserves_ambiguity() {
    let temporary = tempfile::tempdir().unwrap();
    let database = temporary.path().join("relations.sqlite");
    let mut connection = open_database_at(&database).unwrap();
    let scan = graph_resolution_scan("D:/relations");
    let project_id = write_index(&mut connection, &scan).unwrap();

    let calls: Vec<(u64, String, String, String)> = connection
        .prepare(
            "SELECT line, resolved_target_key, candidate_target_keys_json, confidence
             FROM calls WHERE project_id = ?1 ORDER BY line",
        )
        .unwrap()
        .query_map([project_id], |row| {
            Ok((
                row.get::<_, i64>(0)? as u64,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
            ))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(
        calls,
        vec![
            (
                10,
                "sym:0000000000000003".to_string(),
                "[\"sym:0000000000000003\"]".to_string(),
                "resolved".to_string(),
            ),
            (
                20,
                String::new(),
                "[\"sym:0000000000000003\",\"sym:0000000000000004\"]".to_string(),
                "ambiguous".to_string(),
            ),
            (
                30,
                String::new(),
                "[]".to_string(),
                "unresolved".to_string()
            ),
        ]
    );

    let relations: Vec<(String, String, String, i64)> = connection
        .prepare(
            "SELECT relation_kind, target_key, confidence, ambiguous
             FROM symbol_relations
             WHERE project_id = ?1 AND relation_kind != 'calls'
             ORDER BY evidence_line",
        )
        .unwrap()
        .query_map([project_id], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(
        relations,
        vec![
            (
                "inherits".to_string(),
                "sym:0000000000000006".to_string(),
                "resolved".to_string(),
                0,
            ),
            (
                "uses".to_string(),
                String::new(),
                "unresolved".to_string(),
                0
            ),
        ]
    );

    let module_edge: (i64, i64, String) = connection
        .query_row(
            "SELECT weight, evidence_count, confidence FROM module_relations
             WHERE project_id = ?1 AND source_module = 'module:app'
               AND target_module = 'module:util' AND relation_kind = 'include'",
            [project_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(module_edge, (3, 3, "resolved".to_string()));

    let candidates_before: String = connection
        .query_row(
            "SELECT candidate_target_keys_json FROM calls
             WHERE project_id = ?1 AND line = 20",
            [project_id],
            |row| row.get(0),
        )
        .unwrap();
    write_index(&mut connection, &scan).unwrap();
    let candidates_after: String = connection
        .query_row(
            "SELECT candidate_target_keys_json FROM calls
             WHERE project_id = ?1 AND line = 20",
            [project_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(candidates_after, candidates_before);
}

#[test]
fn graph_query_returns_all_five_normalized_graph_kinds() {
    let temporary = tempfile::tempdir().unwrap();
    let database = temporary.path().join("five-kinds.sqlite");
    let mut connection = open_database_at(&database).unwrap();
    let project_id = write_index(&mut connection, &graph_resolution_scan("D:/five-kinds")).unwrap();

    let modules = query_graph(
        &connection,
        graph_request(project_id, GraphKind::ModuleDependencies, &[]),
    )
    .unwrap();
    assert!(modules.edges.iter().any(|edge| {
        edge.source == "module:app"
            && edge.target == "module:util"
            && edge.kind == "include"
            && edge.weight == 3
    }));

    let calls = query_graph(
        &connection,
        graph_request(
            project_id,
            GraphKind::FunctionCalls,
            &["sym:0000000000000001"],
        ),
    )
    .unwrap();
    assert!(calls
        .nodes
        .iter()
        .any(|node| node.id == "sym:0000000000000003"));
    assert!(calls.edges.iter().any(|edge| {
        edge.source == "sym:0000000000000001"
            && edge.target == "sym:0000000000000003"
            && edge.kind == "calls"
    }));

    let classes = query_graph(
        &connection,
        graph_request(
            project_id,
            GraphKind::ClassRelations,
            &["sym:0000000000000005"],
        ),
    )
    .unwrap();
    assert!(classes.edges.iter().any(|edge| {
        edge.source == "sym:0000000000000005"
            && edge.target == "sym:0000000000000006"
            && edge.kind == "inherits"
    }));

    let includes = query_graph(
        &connection,
        graph_request(project_id, GraphKind::FileIncludes, &["file:app/main.cpp"]),
    )
    .unwrap();
    assert!(includes
        .nodes
        .iter()
        .all(|node| node.id.starts_with("file:")));
    assert!(includes
        .nodes
        .iter()
        .any(|node| node.id == "file:util/one.h"));

    let mut entry_request =
        graph_request(project_id, GraphKind::EntryPaths, &["sym:0000000000000001"]);
    entry_request.direction = GraphDirection::Outgoing;
    entry_request.path_target = Some("sym:0000000000000003".to_string());
    let entries = query_graph(&connection, entry_request).unwrap();
    assert_eq!(
        entries.statistics.shortest_path_node_ids,
        [
            "sym:0000000000000001".to_string(),
            "sym:0000000000000003".to_string()
        ]
    );
}

fn synthetic_function(key: usize, name: &str) -> SymbolInfo {
    SymbolInfo {
        file: format!("chain/{name}.cpp"),
        line: 1,
        kind: "function".to_string(),
        name: name.to_string(),
        qualified_name: format!("chain::{name}"),
        stable_key: format!("sym:{key:016x}"),
        parent_qualified_name: "chain".to_string(),
        parent_stable_key: String::new(),
        signature: format!("void chain::{name}()"),
        scope: "chain".to_string(),
        is_definition: true,
        confidence: "heuristic".to_string(),
    }
}

fn synthetic_call(source: usize, target: usize, line: u64) -> CallInfo {
    CallInfo {
        file: format!("chain/node_{source}.cpp"),
        line,
        name: format!("node_{target}"),
        target_qualified_name: format!("chain::node_{target}"),
        caller_qualified_name: format!("chain::node_{source}"),
        caller_stable_key: format!("sym:{source:016x}"),
        confidence: "heuristic".to_string(),
    }
}

#[test]
fn graph_query_honors_direction_depth_and_search_filters() {
    let temporary = tempfile::tempdir().unwrap();
    let database = temporary.path().join("traversal.sqlite");
    let mut connection = open_database_at(&database).unwrap();
    let project_id = write_index(&mut connection, &graph_resolution_scan("D:/traversal")).unwrap();

    let mut outgoing = graph_request(
        project_id,
        GraphKind::FunctionCalls,
        &["sym:0000000000000001"],
    );
    outgoing.direction = GraphDirection::Outgoing;
    outgoing.depth = 0;
    let depth_zero = query_graph(&connection, outgoing.clone()).unwrap();
    assert_eq!(
        depth_zero
            .nodes
            .iter()
            .map(|node| node.id.as_str())
            .collect::<Vec<_>>(),
        ["sym:0000000000000001"]
    );

    outgoing.depth = 1;
    let depth_one = query_graph(&connection, outgoing).unwrap();
    assert!(depth_one
        .nodes
        .iter()
        .any(|node| node.id == "sym:0000000000000003"));
    assert!(!depth_one
        .nodes
        .iter()
        .any(|node| node.id == "sym:0000000000000002"));

    let mut incoming = graph_request(
        project_id,
        GraphKind::FunctionCalls,
        &["sym:0000000000000003"],
    );
    incoming.direction = GraphDirection::Incoming;
    incoming.depth = 1;
    let incoming = query_graph(&connection, incoming).unwrap();
    assert!(incoming
        .nodes
        .iter()
        .any(|node| node.id == "sym:0000000000000001"));

    let mut search = graph_request(project_id, GraphKind::FunctionCalls, &[]);
    search.filters.search = Some("util::run".to_string());
    let search = query_graph(&connection, search).unwrap();
    assert_eq!(search.nodes.len(), 1);
    assert_eq!(search.nodes[0].id, "sym:0000000000000004");
}

#[test]
fn graph_query_reports_cycles_paths_and_ambiguous_evidence() {
    let temporary = tempfile::tempdir().unwrap();
    let database = temporary.path().join("insights.sqlite");
    let mut connection = open_database_at(&database).unwrap();
    let mut scan = graph_resolution_scan("D:/insights");
    scan.symbols.extend([
        synthetic_function(7, "node_7"),
        synthetic_function(8, "node_8"),
        synthetic_function(9, "node_9"),
    ]);
    scan.calls.extend([
        synthetic_call(7, 8, 1),
        synthetic_call(8, 9, 2),
        synthetic_call(9, 7, 3),
    ]);
    let project_id = write_index(&mut connection, &scan).unwrap();

    let cycle = query_graph(
        &connection,
        graph_request(
            project_id,
            GraphKind::FunctionCalls,
            &["sym:0000000000000007"],
        ),
    )
    .unwrap();
    assert_eq!(cycle.statistics.cycle_count, 1);
    assert!(cycle.groups.iter().any(|group| {
        group.kind == "cycle"
            && group.node_ids
                == [
                    "sym:0000000000000007".to_string(),
                    "sym:0000000000000008".to_string(),
                    "sym:0000000000000009".to_string(),
                ]
    }));

    let mut disconnected =
        graph_request(project_id, GraphKind::EntryPaths, &["sym:0000000000000001"]);
    disconnected.direction = GraphDirection::Outgoing;
    disconnected.path_target = Some("sym:0000000000000004".to_string());
    assert!(query_graph(&connection, disconnected)
        .unwrap()
        .statistics
        .shortest_path_node_ids
        .is_empty());

    let ambiguous = query_graph(
        &connection,
        graph_request(
            project_id,
            GraphKind::FunctionCalls,
            &["sym:0000000000000002"],
        ),
    )
    .unwrap();
    let edge = ambiguous.edges.iter().find(|edge| edge.ambiguous).unwrap();
    assert_eq!(
        edge.confidence,
        cppinsight_ai_lib::graph::GraphConfidence::Ambiguous
    );
    assert!(!edge.evidence_ids.is_empty());
    assert!(edge.evidence_ids.iter().all(|id| id.starts_with("ev:")));
    assert!(ambiguous
        .evidence
        .iter()
        .any(|evidence| evidence.edge_id == edge.id));
    let mut resolved_only = graph_request(
        project_id,
        GraphKind::FunctionCalls,
        &["sym:0000000000000002"],
    );
    resolved_only.filters.confidence = vec![cppinsight_ai_lib::graph::GraphConfidence::Resolved];
    let resolved_only = query_graph(&connection, resolved_only).unwrap();
    assert!(resolved_only.edges.iter().all(|edge| !edge.ambiguous));
    assert!(resolved_only
        .nodes
        .iter()
        .all(|node| !node.id.starts_with("unresolved:")));

    let mut missing_target =
        graph_request(project_id, GraphKind::EntryPaths, &["sym:0000000000000001"]);
    missing_target.path_target = Some("sym:ffffffffffffffff".to_string());
    assert!(query_graph(&connection, missing_target).is_err());
}

#[test]
fn graph_query_continuations_cover_large_graph_deterministically() {
    let temporary = tempfile::tempdir().unwrap();
    let database = temporary.path().join("pagination.sqlite");
    let mut connection = open_database_at(&database).unwrap();
    let mut scan = graph_resolution_scan("D:/pagination");
    scan.symbols.clear();
    scan.calls.clear();
    scan.relations.clear();
    scan.entry_points.clear();
    for index in 1..=620 {
        scan.symbols
            .push(synthetic_function(index, &format!("node_{index}")));
        if index < 620 {
            scan.calls
                .push(synthetic_call(index, index + 1, index as u64));
        }
    }
    let project_id = write_index(&mut connection, &scan).unwrap();
    let roots = (1..=620)
        .map(|index| format!("sym:{index:016x}"))
        .collect::<Vec<_>>();
    let root_refs = roots.iter().map(String::as_str).collect::<Vec<_>>();
    let mut request = graph_request(project_id, GraphKind::FunctionCalls, &root_refs);
    request.direction = GraphDirection::Outgoing;
    request.depth = 8;
    let first = query_graph(&connection, request.clone()).unwrap();
    assert_eq!(first.nodes.len(), 500);
    assert!(first.truncated);
    assert_eq!(first.truncation_reason.as_deref(), Some("node_limit"));
    assert_eq!(first.statistics.original_node_count, 620);
    let first_ids = first
        .nodes
        .iter()
        .map(|node| node.id.as_str())
        .collect::<std::collections::HashSet<_>>();
    assert!(first
        .edges
        .iter()
        .all(|edge| first_ids.contains(edge.source.as_str())
            && first_ids.contains(edge.target.as_str())));

    request.continuation = first.continuation.clone();
    let second = query_graph(&connection, request).unwrap();
    assert_eq!(second.nodes.len(), 120);
    assert!(!second.truncated);
    let second_ids = second
        .nodes
        .iter()
        .map(|node| node.id.as_str())
        .collect::<std::collections::HashSet<_>>();
    assert!(first_ids.is_disjoint(&second_ids));
    assert!(second
        .edges
        .iter()
        .all(|edge| second_ids.contains(edge.source.as_str())
            && second_ids.contains(edge.target.as_str())));
    assert_eq!(first_ids.len() + second_ids.len(), 620);
    assert_eq!(first.edges.len() + second.edges.len(), 618);
}
