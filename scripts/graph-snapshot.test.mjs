import assert from "node:assert/strict";
import test from "node:test";
import { graphSvgSnapshot, hiddenDescendants } from "../desktop/src/graphSnapshot.ts";

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

test("explicit path edges prevent unrelated export highlights", () => {
  const svg = graphSvgSnapshot(nodes, edges, positions, new Set(nodes.map(node => node.id)), new Set(), new Set());
  assert.doesNotMatch(svg, /stroke="#ffd166"/);
});

test("graph SVG snapshot covers every first-release graph kind", () => {
  const graphKinds = [
    ["module_dependencies", "module", "depends_on"],
    ["function_calls", "function", "calls"],
    ["class_relations", "class", "inherits"],
    ["file_includes", "file", "includes"],
    ["entry_paths", "function", "entry_path"]
  ];

  for (const [graphKind, nodeKind, edgeKind] of graphKinds) {
    const source = {
      id: `sym:${graphKind}:source`,
      kind: nodeKind,
      label: `${graphKind}:A`,
      qualified_name: `fixture::${graphKind}::source`,
      file: `src/${graphKind}.cpp`,
      line: 1,
      confidence: "precise",
      ambiguous: false,
      metadata: {}
    };
    const target = {
      ...source,
      id: `sym:${graphKind}:target`,
      label: `${graphKind}:B`,
      qualified_name: `fixture::${graphKind}::target`,
      line: 2
    };
    const svg = graphSvgSnapshot(
      [source, target],
      [{
        id: `edge:${graphKind}`,
        source: source.id,
        target: target.id,
        kind: edgeKind,
        weight: 1,
        confidence: "precise",
        evidence_ids: []
      }],
      {
        [source.id]: { x: 120, y: 80 },
        [target.id]: { x: 420, y: 260 }
      },
      new Set(),
      new Set()
    );

    assert.match(svg, new RegExp(`${graphKind}:A`));
    assert.match(svg, new RegExp(`${graphKind}:B`));
    assert.equal((svg.match(/<circle /g) ?? []).length, 2);
    assert.equal((svg.match(/<line /g) ?? []).length, 1);
  }
});

test("collapsed nodes hide only downstream descendants", () => {
  const hidden = hiddenDescendants(edges.concat({
    id: "edge:bbbbbbbbbbbbbbbb",
    source: nodes[1].id,
    target: "sym:cccccccccccccccc",
    kind: "uses",
    weight: 1,
    confidence: "heuristic",
    evidence_ids: []
  }), new Set([nodes[0].id]));
  assert.deepEqual([...hidden].sort(), [nodes[1].id, "sym:cccccccccccccccc"].sort());
});
