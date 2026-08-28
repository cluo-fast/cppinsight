use cppinsight_ai_lib::graph::{
    GraphDirection, GraphKind, GraphQueryRequest, GraphQueryResponse, GRAPH_PROTOCOL_VERSION,
};
use serde_json::{json, Value};

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
