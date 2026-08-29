import assert from "node:assert/strict";
import test from "node:test";
import { graphSvgSnapshot } from "../desktop/src/graphSnapshot.ts";

const nodes = [
  {
    id: "sym:aaaaaaaaaaaaaaaa",
    kind: "function",
    label: "main <入口>",
    qualified_name: "app::main",
    file: "src/main.cpp",
    line: 4,
    confidence: "precise",
    ambiguous: false,
    metadata: {}
  },
  {
    id: "sym:bbbbbbbbbbbbbbbb",
    kind: "class",
    label: "Runner & Worker",
    qualified_name: "app::Runner",
    file: "src/runner.hpp",
    line: 8,
    confidence: "heuristic",
    ambiguous: false,
    metadata: {}
  }
];

const edges = [{
  id: "edge:aaaaaaaaaaaaaaaa",
  source: nodes[0].id,
  target: nodes[1].id,
  kind: "uses",
  weight: 2,
  confidence: "precise",
  evidence_ids: []
}];

const positions = {
  [nodes[0].id]: { x: 120, y: 80 },
  [nodes[1].id]: { x: 420, y: 260 }
};

test("graph SVG snapshot is deterministic and escapes labels", () => {
  const first = graphSvgSnapshot(nodes, edges, positions, new Set(), new Set());
  const second = graphSvgSnapshot(nodes, edges, positions, new Set(), new Set());
  assert.equal(first, second);
  assert.match(first, /^<svg xmlns="http:\/\/www\.w3\.org\/2000\/svg"/);
  assert.equal((first.match(/<circle /g) ?? []).length, 2);
  assert.equal((first.match(/<line /g) ?? []).length, 1);
  assert.match(first, /main &lt;入口&gt;/);
  assert.match(first, /Runner &amp; Worker/);
  assert.doesNotMatch(first, /main <入口>/);
});

test("graph SVG snapshot highlights path and central nodes", () => {
  const svg = graphSvgSnapshot(
    nodes,
    edges,
    positions,
    new Set([nodes[0].id, nodes[1].id]),
    new Set([nodes[0].id])
  );
  assert.match(svg, /stroke="#ffd166"/);
  assert.match(svg, /r="17"/);
});
