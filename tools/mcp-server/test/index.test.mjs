import assert from "node:assert/strict";
import test from "node:test";
import { handleToolCall, listToolDefinitions } from "../core.mjs";

test("cppinsight_graph advertises bounded protocol fields", () => {
  const graph = listToolDefinitions().find((tool) => tool.name === "cppinsight_graph");
  assert.ok(graph);
  assert.deepEqual(graph.inputSchema.properties.graphKind.enum, [
    "module_dependencies",
    "function_calls",
    "class_relations",
    "file_includes",
    "entry_paths"
  ]);
  assert.equal(graph.inputSchema.properties.maxNodes.maximum, 500);
});

test("cppinsight_graph forwards request JSON through stdin without clamping", async () => {
  const calls = [];
  const runner = async (args, options) => {
    calls.push({ args, options });
    return '{"protocol_version":1,"nodes":[]}';
  };
  const response = await handleToolCall(
    {
      params: {
        name: "cppinsight_graph",
        arguments: {
          projectPath: "D:/repo with spaces",
          graphKind: "function_calls",
          rootNodeIds: ["sym:0000000000000001"],
          direction: "outgoing",
          depth: 7,
          filters: {
            moduleIds: ["module:app"],
            filePaths: ["src/main.cpp"],
            symbolKinds: ["function"],
            relationKinds: ["calls"],
            confidence: ["resolved", "ambiguous"],
            search: "run with spaces"
          },
          maxNodes: 17,
          pathTarget: "sym:0000000000000002",
          continuation: "v1:10"
        }
      }
    },
    runner
  );

  assert.equal(response.isError, undefined);
  assert.deepEqual(calls[0].args, [
    "graph",
    "--root",
    "D:/repo with spaces",
    "--request",
    "-"
  ]);
  const protocol = JSON.parse(calls[0].options.stdin);
  assert.equal(protocol.project_id, 0);
  assert.equal(protocol.max_nodes, 17);
  assert.equal(protocol.depth, 7);
  assert.equal(protocol.filters.search, "run with spaces");
  assert.deepEqual(protocol.root_node_ids, ["sym:0000000000000001"]);
});

test("cppinsight_graph propagates workbench validation errors", async () => {
  const response = await handleToolCall(
    {
      params: {
        name: "cppinsight_graph",
        arguments: {
          projectPath: "D:/repo",
          graphKind: "function_calls"
        }
      }
    },
    async () => {
      throw new Error("graph query validation error: depth must be at most 8");
    }
  );
  assert.equal(response.isError, true);
  assert.match(response.content[0].text, /depth must be at most 8/);
});
