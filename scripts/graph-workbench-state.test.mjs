import assert from "node:assert/strict";
import test from "node:test";

const state = await import("../desktop/src/graphWorkbenchState.ts");

test("graph request builder preserves UI filters and pagination", () => {
  const request = state.createGraphRequest(
    7,
    "function_calls",
    "incoming",
    3,
    "  Engine  ",
    "precise",
    "calls, inherits, calls",
    "v1:10",
    ["sym:engine"],
    "sym:target"
  );
  assert.deepEqual(request, {
    protocol_version: 1,
    project_id: 7,
    graph_kind: "function_calls",
    root_node_ids: ["sym:engine"],
    direction: "incoming",
    depth: 3,
    filters: {
      module_ids: [],
      file_paths: [],
      symbol_kinds: [],
      relation_kinds: ["calls", "inherits", "calls"],
      confidence: ["precise"],
      search: "Engine"
    },
    max_nodes: 500,
    path_target: "sym:target",
    continuation: "v1:10"
  });
});

test("graph response merger keeps unique entities while adopting the latest page", () => {
  const first = {
    nodes: [{ id: "a" }, { id: "b" }],
    edges: [{ id: "e1", source: "a", target: "b" }],
    evidence: [{ id: "ev1" }],
    continuation: "v1:2",
    statistics: { page: 1 }
  };
  const second = {
    nodes: [{ id: "b", label: "updated" }, { id: "c" }],
    edges: [{ id: "e1", source: "a", target: "b" }, { id: "e2", source: "b", target: "c" }],
    evidence: [{ id: "ev1" }, { id: "ev2" }],
    continuation: null,
    statistics: { page: 2 }
  };
  assert.deepEqual(state.mergeGraphResponses(first, second), {
    ...second,
    nodes: [{ id: "a" }, { id: "b", label: "updated" }, { id: "c" }],
    edges: [
      { id: "e1", source: "a", target: "b" },
      { id: "e2", source: "b", target: "c" }
    ],
    evidence: [{ id: "ev1" }, { id: "ev2" }]
  });
});
