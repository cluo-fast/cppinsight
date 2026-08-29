import { readFileSync } from "node:fs";
import path from "node:path";
import process from "node:process";

const expectedKinds = [
  "module_dependencies",
  "function_calls",
  "class_relations",
  "file_includes",
  "entry_paths"
];
const failures = [];

function fail(message) {
  failures.push(message);
}

function isFiniteNonNegative(value) {
  return typeof value === "number" && Number.isFinite(value) && value >= 0;
}

function isSafeBasename(value, label) {
  if (typeof value !== "string" || value.length === 0) {
    fail(`${label} must be a non-empty string`);
    return;
  }
  if (value !== path.basename(value) || /[\\/:]/u.test(value) || value === "." || value === "..") {
    fail(`${label} must be a basename without a path`);
  }
}

function expectedMedian(samples) {
  const sorted = [...samples].sort((left, right) => left - right);
  const middle = Math.floor(sorted.length / 2);
  if (sorted.length % 2 === 1) {
    // Mirrors benchmark-graph.ps1: odd-count medians retain the stopwatch value.
    return sorted[middle];
  }
  const median = (sorted[middle - 1] + sorted[middle]) / 2;
  return Math.round(median * 100) / 100;
}

const inputPath = process.argv[2];
if (!inputPath) {
  console.error("usage: node scripts/verify-benchmark-result.mjs <benchmark-result.json>");
  process.exit(2);
}

let report;
try {
  report = JSON.parse(readFileSync(inputPath, "utf8"));
} catch (error) {
  console.error(`failed to read benchmark result: ${error instanceof Error ? error.message : String(error)}`);
  process.exit(2);
}

if (!report || typeof report !== "object" || Array.isArray(report)) {
  fail("benchmark result must be a JSON object");
} else {
  if (typeof report.generated_at !== "string" || Number.isNaN(Date.parse(report.generated_at))) {
    fail("generated_at must be an ISO date string");
  }
  isSafeBasename(report.project_name, "project_name");
  isSafeBasename(report.workbench_name, "workbench_name");
  if (!Array.isArray(report.results)) {
    fail("results must be an array");
  } else {
    const kinds = report.results.map((item) => item?.graph_kind);
    if (report.results.length !== expectedKinds.length || expectedKinds.some((kind) => !kinds.includes(kind))) {
      fail("results must contain exactly the five graph kinds");
    }
    const seen = new Set();
    report.results.forEach((item, index) => {
      const prefix = `results[${index}]`;
      if (!item || typeof item !== "object" || Array.isArray(item)) {
        fail(`${prefix} must be an object`);
        return;
      }
      if (seen.has(item.graph_kind)) fail(`${prefix}.graph_kind must be unique`);
      seen.add(item.graph_kind);
      if (!Number.isInteger(item.iterations) || item.iterations < 1) {
        fail(`${prefix}.iterations must be a positive integer`);
      }
      for (const key of ["samples_ms", "peak_working_set_mb_samples"]) {
        if (!Array.isArray(item[key]) || item[key].length !== item.iterations || !item[key].every(isFiniteNonNegative)) {
          fail(`${prefix}.${key} must contain one non-negative number per iteration`);
        }
      }
      if (Array.isArray(item.samples_ms) && item.samples_ms.length > 0 &&
          (!isFiniteNonNegative(item.median_ms) || item.median_ms !== expectedMedian(item.samples_ms))) {
        fail(`${prefix}.median_ms must equal the rounded median of samples_ms`);
      }
      if (!isFiniteNonNegative(item.peak_working_set_mb)) {
        fail(`${prefix}.peak_working_set_mb must be a non-negative number`);
      }
      for (const key of ["original_nodes", "original_edges", "returned_nodes", "returned_edges"]) {
        if (!Number.isInteger(item[key]) || item[key] < 0) fail(`${prefix}.${key} must be a non-negative integer`);
      }
      if (item.returned_nodes > item.original_nodes || item.returned_edges > item.original_edges) {
        fail(`${prefix} returned counts cannot exceed original counts`);
      }
      if (typeof item.truncated !== "boolean") fail(`${prefix}.truncated must be boolean`);
      if (item.truncated && item.truncation_reason !== "node_limit") {
        fail(`${prefix}.truncation_reason must be node_limit when truncated`);
      }
      if (!item.truncated && item.truncation_reason !== null) {
        fail(`${prefix}.truncation_reason must be null when not truncated`);
      }
    });
  }
}

if (failures.length > 0) {
  console.error(failures.join("\n"));
  process.exit(1);
}

console.log("benchmark result is valid");
