import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { cpSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { summarizeImpact } from "../desktop/src/impactAnalysis.ts";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
function argument(name) {
  const index = process.argv.indexOf(name);
  if (index < 0 || !process.argv[index + 1]) throw new Error(`Missing ${name}`);
  return path.resolve(process.argv[index + 1]);
}
const scanner = argument("--scanner");
const workbench = argument("--workbench");
const temporary = mkdtempSync(path.join(tmpdir(), "cppinsight-investigation-"));
function run(binary, args, input) {
  return execFileSync(binary, args, { encoding: "utf8", input, timeout: 60000, maxBuffer: 4 * 1024 * 1024 });
}
try {
  const project = path.join(temporary, "sample");
  cpSync(path.join(root, "examples/sample-cpp"), project, { recursive: true,
    filter: source => !source.split(path.sep).includes(".cppinsight") });
  const scan = path.join(temporary, "scan.json");
  const db = path.join(project, ".cppinsight/index.sqlite");
  run(scanner, ["scan", project, "--json", scan, "--quiet"]);
  run(workbench, ["index", scan, "--db", db]);
  function query(kind, roots = [], direction = "both") {
    return JSON.parse(run(workbench, ["graph", "--root", project, "--request", "-"], JSON.stringify({
      protocol_version: 1, project_id: 0, graph_kind: kind, root_node_ids: roots,
      direction, depth: 8, filters: {}, max_nodes: 500, path_target: null, continuation: null
    })));
  }
  for (const kind of ["module_dependencies", "function_calls", "class_relations", "file_includes", "entry_paths"]) {
    const graph = query(kind);
    assert.ok(graph.nodes.length > 0 && graph.edges.length > 0, `${kind}: expected nodes and edges`);
    assert.equal(graph.truncated, false);
  }
  const calls = query("function_calls");
  const target = calls.nodes.find(node => node.qualified_name === "sample::Engine::run");
  assert.ok(target, "sample::Engine::run must be indexed");
  const incoming = query("entry_paths", [target.id], "incoming");
  const summary = summarizeImpact(incoming, target.id);
  assert.ok(summary.entries.some(item => item.node.label === "main" || item.node.qualified_name === "main"), "main must be a affected entry candidate");
  assert.ok(summary.items.every(item => item.path.at(-1).id === target.id));
  assert.ok(summary.items.some(item => item.edges.some(edge => edge.evidence_ids.some(id => incoming.evidence.some(evidence => evidence.id === id)))), "impact needs returned source evidence");
  const report = run(workbench, ["summary", "--root", project]);
  assert.ok(report.includes("main"), "onboarding report should identify the entry");
  console.log("Investigation smoke passed: scanner → index → five graphs → upstream entry/evidence → report.");
} finally {
  rmSync(temporary, { recursive: true, force: true });
}
