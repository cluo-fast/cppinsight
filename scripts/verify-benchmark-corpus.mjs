import { readFileSync } from "node:fs";
import process from "node:process";

const expectedSizeClasses = new Set(["small", "medium", "large"]);
const failures = [];

function fail(message) {
  failures.push(message);
}

const inputPath = process.argv[2] ?? "docs/benchmarks/corpus.json";
let manifest;
try {
  manifest = JSON.parse(readFileSync(inputPath, "utf8"));
} catch (error) {
  console.error(`failed to read benchmark corpus: ${error instanceof Error ? error.message : String(error)}`);
  process.exit(2);
}

if (!manifest || typeof manifest !== "object" || Array.isArray(manifest)) {
  fail("corpus manifest must be an object");
} else {
  if (manifest.schema_version !== 1) fail("schema_version must be 1");
  if (!Number.isInteger(manifest.minimum_projects) || manifest.minimum_projects < 10) {
    fail("minimum_projects must be at least 10");
  }
  if (!Array.isArray(manifest.projects) || manifest.projects.length < manifest.minimum_projects) {
    fail("projects must contain at least minimum_projects entries");
  } else {
    const ids = new Set();
    for (const [index, project] of manifest.projects.entries()) {
      const prefix = `projects[${index}]`;
      if (!project || typeof project !== "object" || Array.isArray(project)) {
        fail(`${prefix} must be an object`);
        continue;
      }
      if (typeof project.id !== "string" || !/^[a-z0-9-]+$/u.test(project.id)) fail(`${prefix}.id must be a slug`);
      if (ids.has(project.id)) fail(`${prefix}.id must be unique`);
      ids.add(project.id);
      if (typeof project.repository !== "string" || !/^https:\/\/github\.com\/[^/]+\/[^/]+\.git$/u.test(project.repository)) {
        fail(`${prefix}.repository must be a canonical GitHub HTTPS URL`);
      }
      if (typeof project.license_spdx !== "string" || project.license_spdx.length === 0) fail(`${prefix}.license_spdx is required`);
      if (typeof project.build_system !== "string" || project.build_system.length === 0) fail(`${prefix}.build_system is required`);
      if (!expectedSizeClasses.has(project.size_class)) fail(`${prefix}.size_class must be small, medium, or large`);
      if (project.revision !== null && (typeof project.revision !== "string" || !/^[0-9a-f]{7,64}$/iu.test(project.revision))) {
        fail(`${prefix}.revision must be null or a commit SHA`);
      }
      if (!Array.isArray(project.accuracy_cases)) fail(`${prefix}.accuracy_cases must be an array`);
    }
  }
}

if (failures.length > 0) {
  console.error(failures.join("\n"));
  process.exit(1);
}

console.log(`benchmark corpus is valid (${manifest.projects.length} projects)`);
