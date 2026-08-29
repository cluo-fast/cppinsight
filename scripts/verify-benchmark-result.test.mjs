import assert from "node:assert/strict";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import test from "node:test";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const verifier = path.join(root, "scripts", "verify-benchmark-result.mjs");

const validResult = {
  generated_at: "2026-08-29T00:00:00.000Z",
  project_name: "sample-cpp",
  workbench_name: "cppinsight-workbench.exe",
  results: [
    "module_dependencies",
    "function_calls",
    "class_relations",
    "file_includes",
    "entry_paths"
  ].map((graph_kind) => ({
    graph_kind,
    iterations: 3,
    samples_ms: [10, 12, 11],
    median_ms: 11,
    peak_working_set_mb_samples: [42, 43, 42],
    peak_working_set_mb: 43,
    original_nodes: 4,
    original_edges: 3,
    returned_nodes: 4,
    returned_edges: 3,
    truncated: false,
    truncation_reason: null
  }))
};

function runVerifier(value) {
  const directory = mkdtempSync(path.join(tmpdir(), "cppinsight-benchmark-test-"));
  const file = path.join(directory, "result.json");
  writeFileSync(file, JSON.stringify(value));
  const result = spawnSync(process.execPath, [verifier, file], { encoding: "utf8" });
  rmSync(directory, { recursive: true, force: true });
  return result;
}

test("benchmark verifier accepts a complete private-safe result", () => {
  const result = runVerifier(validResult);
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /benchmark result is valid/);
});

test("benchmark verifier rejects path leaks and missing graph kinds", () => {
  const leaked = structuredClone(validResult);
  leaked.project_name = "C:\\Users\\alice\\project";
  leaked.results = leaked.results.slice(0, 4);
  const result = runVerifier(leaked);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /project_name must be a basename|exactly the five/);
});

test("benchmark verifier mirrors the PowerShell median rules", () => {
  const roundedEven = structuredClone(validResult);
  roundedEven.results[0].iterations = 2;
  roundedEven.results[0].samples_ms = [10.12, 10.13];
  roundedEven.results[0].peak_working_set_mb_samples = [42, 43];
  roundedEven.results[0].median_ms = 10.13;
  const evenResult = runVerifier(roundedEven);
  assert.equal(evenResult.status, 0, evenResult.stderr);

  const rawOdd = structuredClone(validResult);
  rawOdd.results[0].samples_ms = [10.12, 10.13, 10.14];
  rawOdd.results[0].median_ms = 10.13;
  const oddResult = runVerifier(rawOdd);
  assert.equal(oddResult.status, 0, oddResult.stderr);
});
