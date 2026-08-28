use cppinsight_ai_lib::graph::{
    GraphDirection, GraphKind, GraphQueryRequest, GraphQueryResponse, GRAPH_PROTOCOL_VERSION,
};
use cppinsight_ai_lib::index::{load_index_status, INDEX_SCHEMA_VERSION};
use cppinsight_ai_lib::{
    open_database_at, write_index, CallInfo, LineStats, ScanResult, ScanStats, SymbolInfo,
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
