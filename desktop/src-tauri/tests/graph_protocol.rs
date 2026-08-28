use cppinsight_ai_lib::graph::{
    GraphDirection, GraphKind, GraphQueryRequest, GraphQueryResponse, GRAPH_PROTOCOL_VERSION,
};
use cppinsight_ai_lib::index::{load_index_status, INDEX_SCHEMA_VERSION};
use cppinsight_ai_lib::{
    open_database_at, write_index, CallInfo, IncludeInfo, LineStats, ScanResult, ScanStats,
    SymbolInfo, SymbolRelationInfo,
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
        entry_points: Vec::new(),
        files: Vec::new(),
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
        entry_points: Vec::new(),
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
