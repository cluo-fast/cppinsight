import assert from "node:assert/strict";
import test from "node:test";
import { summarizeImpact, pathEdgeIds } from "../desktop/src/impactAnalysis.ts";

const node = (id, kind = "function") => ({ id, kind, label: "sameName", file: `${id}.cpp`, module_id: "app", ambiguous: false });
const edge = (source, target, confidence = "resolved") => ({ id: `${source}-${target}`, source, target, kind: "calls", confidence, ambiguous: false, evidence_ids: [`ev-${source}`] });
const graph = () => ({
  nodes: [node("main", "entry"), node("caller"), node("target"), node("downstream"), node("second", "entry")],
  edges: [edge("main", "caller"), edge("caller", "target", "heuristic"), edge("target", "downstream"), edge("target", "caller"), edge("target", "target"), edge("second", "target")],
  evidence: [], truncated: false, statistics: { original_node_count: 5 }
});
test("impact follows callers, separates direct and indirect, preserves evidence and entry identities", () => {
  const result = summarizeImpact(graph(), "target");
  assert.deepEqual(result.direct.map(x => x.node.id), ["caller", "second"]);
  assert.deepEqual(result.indirect.map(x => x.node.id), ["main"]);
  assert.deepEqual(result.entries.map(x => x.node.id), ["second", "main"]);
  const main = result.items.find(x => x.node.id === "main");
  assert.deepEqual(main.path.map(x => x.id), ["main", "caller", "target"]);
  assert.equal(main.uncertain, true);
  assert.deepEqual(main.edges[1].evidence_ids, ["ev-caller"]);
  assert.equal(result.items.some(x => x.node.id === "downstream"), false);
});
test("missing root and pagination never imply complete absence of impact", () => {
  const data = graph(); data.truncated = true;
  const result = summarizeImpact(data, "not-on-page");
  assert.equal(result.root, undefined);
  assert.deepEqual(result.items, []);
  assert.equal(result.partial, true);
});
test("uncertainty propagates through an ambiguous hop; input ordering cannot change shortest path", () => {
  const data = graph(); data.edges.find(x => x.source === "second").ambiguous = true;
  const before = summarizeImpact(data, "target");
  assert.equal(before.items.find(x => x.node.id === "second").uncertain, true);
  data.edges.reverse();
  assert.deepEqual(summarizeImpact(data, "target"), before);
});
test("path highlighting excludes shortcuts and unrelated edges between path nodes", () => {
  const edges = [edge("a", "b"), edge("b", "c"), edge("a", "c"), edge("c", "c")];
  assert.deepEqual([...pathEdgeIds(edges, ["c", "b", "a"])], ["a-b", "b-c"]);
});

test("directed path highlights only the actual traversal direction", () => {
  const edges = [edge("a", "b"), edge("b", "a")];
  assert.deepEqual([...pathEdgeIds(edges, ["a", "b"], "outgoing")], ["a-b"]);
  assert.deepEqual([...pathEdgeIds(edges, ["a", "b"], "incoming")], ["b-a"]);
});
