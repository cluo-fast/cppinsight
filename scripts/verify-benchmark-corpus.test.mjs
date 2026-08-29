import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const verifier = path.join(root, "scripts", "verify-benchmark-corpus.mjs");
const manifest = path.join(root, "docs", "benchmarks", "corpus.json");

test("benchmark corpus contains the required ten-project candidate set", () => {
  const result = spawnSync(process.execPath, [verifier, manifest], { encoding: "utf8" });
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /benchmark corpus is valid \(10 projects\)/);
});
