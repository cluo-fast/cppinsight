import React from "react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { act, create } from "react-test-renderer";
import ImpactPanel from "../src/ImpactPanel";
import SourcePreviewPanel from "../src/SourcePreviewPanel";
import GraphWorkbench from "../src/GraphWorkbench";
import * as api from "../src/api";

vi.mock("../src/api", () => ({
  queryGraph: vi.fn(), previewSource: vi.fn(), loadGraphLayout: vi.fn(), saveGraphLayout: vi.fn()
}));
// Native canvas rendering is tested separately in the desktop. Keep the fallback for these interaction tests.
vi.mock("cytoscape", () => { throw new Error("no canvas in component test"); });
const target = { id: "sym:0000000000000001", kind: "function", label: "run", qualified_name: "app::run", file: "run.cpp", line: 10, module_id: "app", confidence: "resolved", ambiguous: false, metadata: {} };
const caller = { ...target, id: "sym:0000000000000002", kind: "entry", label: "main", qualified_name: "main", file: "main.cpp" };
const response = () => ({
  protocol_version: 1, project_id: 1, graph_kind: "entry_paths", nodes: [target, caller],
  edges: [{ id: "call", source: caller.id, target: target.id, kind: "calls", weight: 1, confidence: "heuristic", ambiguous: false, evidence_ids: ["evidence"] }],
  evidence: [{ id: "evidence", edge_id: "call", file: "main.cpp", line: 12, confidence: "heuristic", detail: "run()" }],
  groups: [], confidence: { overall: "heuristic" },
  statistics: { original_node_count: 2, returned_node_count: 2, original_edge_count: 1, returned_edge_count: 1, central_node_ids: [], shortest_path_node_ids: [], cycle_count: 0 },
  truncated: false, continuation: null, truncation_reason: null
});
let mounted;
const props = () => ({ root: "/project", projectId: 1, target, generation: 1, onClose: vi.fn(), onLocate: vi.fn(), onSource: vi.fn() });
const text = () => JSON.stringify(mounted.toJSON());
const button = label => mounted.root.findAllByType("button").find(node => node.children.join("") === label);
beforeEach(() => {
  vi.resetAllMocks();
  api.queryGraph.mockResolvedValue(response());
  api.loadGraphLayout.mockResolvedValue(null);
});
afterEach(() => { if (mounted) act(() => mounted.unmount()); mounted = null; });

test("impact controls query incoming entry paths and link real evidence", async () => {
  const input = props();
  await act(async () => { mounted = create(<ImpactPanel {...input} />); });
  expect(api.queryGraph).toHaveBeenLastCalledWith("/project", expect.objectContaining({ graph_kind: "entry_paths", direction: "incoming", depth: 3, root_node_ids: [target.id], max_nodes: 500 }));
  expect(text()).toContain("入口候选");
  expect(text()).toContain("含推断或歧义");
  await act(async () => button("main.cpp:12").props.onClick());
  expect(input.onSource).toHaveBeenCalledWith("main.cpp", 12);
  await act(async () => button("main").props.onClick());
  expect(input.onLocate).toHaveBeenCalledWith(caller);
  await act(async () => mounted.root.findByType("select").props.onChange({ target: { value: "5" } }));
  expect(api.queryGraph.mock.lastCall[1].depth).toBe(5);
});

test("old impact response cannot overwrite a new project or index generation", async () => {
  let finishOld;
  api.queryGraph.mockImplementationOnce(() => new Promise(resolve => { finishOld = resolve; }));
  const input = props();
  await act(async () => { mounted = create(<ImpactPanel {...input} />); });
  api.queryGraph.mockResolvedValue({ ...response(), nodes: [target], edges: [] });
  await act(async () => mounted.update(<ImpactPanel {...input} root="/new-project" projectId={2} generation={2} />));
  await act(async () => finishOld(response()));
  expect(text()).toContain("未发现上游调用");
  expect(text()).not.toContain("main.cpp");
});

test("truncated impact explicitly warns; rejected requests surface an error", async () => {
  api.queryGraph.mockResolvedValue({ ...response(), truncated: true });
  const input = props();
  await act(async () => { mounted = create(<ImpactPanel {...input} />); });
  expect(text()).toContain("不是完整影响范围");
  api.queryGraph.mockRejectedValue(new Error("index missing"));
  await act(async () => mounted.update(<ImpactPanel {...input} generation={2} />));
  expect(text()).toContain("影响查询失败");
  expect(text()).not.toContain("main.cpp");
});

test("source preview renders HTML as text, marks focus, and discards stale requests", async () => {
  let finishOld;
  api.previewSource.mockImplementationOnce(() => new Promise(resolve => { finishOld = resolve; }));
  const input = { root: "/project", projectId: 1, file: "old.cpp", line: 1, generation: 1, onClose: vi.fn() };
  await act(async () => { mounted = create(<SourcePreviewPanel {...input} />); });
  api.previewSource.mockResolvedValue({ file: "new.cpp", start_line: 8, end_line: 9, focus_line: 9, text: "line 8\n<script>alert(1)</script>", indexed_at: "snapshot-time", truncated: true });
  await act(async () => mounted.update(<SourcePreviewPanel {...input} file="new.cpp" line={9} />));
  await act(async () => finishOld({ text: "stale secret", start_line: 1, focus_line: 1 }));
  expect(mounted.root.findAllByType("script")).toHaveLength(0);
  expect(text()).toContain("<script>alert(1)</script>");
  expect(text()).toContain("snapshot-time");
  expect(text()).not.toContain("stale secret");
  expect(mounted.root.findAllByProps({ className: "source-focus" })).toHaveLength(1);
});

test("workbench shortcuts reset filters and project changes close investigation panels", async () => {
  await act(async () => { mounted = create(<GraphWorkbench root="/project" projectId={1} graphDataVersion={1} />); });
  await act(async () => button("改这里会影响哪里").props.onClick());
  expect(mounted.root.findAllByType(ImpactPanel)).toHaveLength(1);
  expect(api.queryGraph.mock.calls.some(([, request]) => request.graph_kind === "function_calls" && request.direction === "incoming" && request.root_node_ids[0] === target.id)).toBe(true);
  await act(async () => mounted.update(<GraphWorkbench root="/other" projectId={2} graphDataVersion={1} />));
  expect(mounted.root.findAllByType(ImpactPanel)).toHaveLength(0);
  expect(mounted.root.findAllByType(SourcePreviewPanel)).toHaveLength(0);
  expect(api.queryGraph.mock.lastCall[0]).toBe("/other");
  expect(api.queryGraph.mock.lastCall[1].root_node_ids).toEqual([]);
});
