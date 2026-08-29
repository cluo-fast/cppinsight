import { createHash } from "node:crypto";
import { createReadStream } from "node:fs";
import { access, readFile } from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const HASH_LINE = /^([a-f0-9]{64})  (.+)$/i;

function fail(message) {
  throw new Error(message);
}

function resolveManifestPath(root, relativePath) {
  const normalized = relativePath.replaceAll("\\", "/");
  if (!normalized || normalized.startsWith("/") || /^[a-z]:\//i.test(normalized)) {
    fail(`unsafe manifest path: ${relativePath}`);
  }
  const segments = normalized.split("/");
  if (segments.some((segment) => segment === "..")) {
    fail(`unsafe manifest path: ${relativePath}`);
  }
  const resolved = path.resolve(root, ...segments);
  const relative = path.relative(root, resolved);
  if (relative === "" || relative.startsWith(`..${path.sep}`) || path.isAbsolute(relative)) {
    fail(`unsafe manifest path: ${relativePath}`);
  }
  return resolved;
}

async function sha256(filePath) {
  const hash = createHash("sha256");
  for await (const chunk of createReadStream(filePath)) {
    hash.update(chunk);
  }
  return hash.digest("hex");
}

/** Verify a SHA256SUMS-style manifest without allowing paths outside its directory. */
export async function verifyManifest(manifestPath) {
  const absoluteManifest = path.resolve(manifestPath);
  const root = path.dirname(absoluteManifest);
  const content = await readFile(absoluteManifest, "utf8");
  const entries = [];
  const seen = new Set();

  for (const [index, rawLine] of content.split(/\r?\n/).entries()) {
    const line = rawLine.trimEnd();
    if (!line.trim()) continue;
    const match = line.match(HASH_LINE);
    if (!match) fail(`invalid manifest line ${index + 1}`);
    const [, expected, relativePath] = match;
    const filePath = resolveManifestPath(root, relativePath);
    const key = path.normalize(filePath);
    if (seen.has(key)) fail(`duplicate manifest entry: ${relativePath}`);
    seen.add(key);
    entries.push({ expected: expected.toLowerCase(), filePath, relativePath });
  }

  if (entries.length === 0) fail("manifest contains no entries");
  for (const entry of entries) {
    try {
      await access(entry.filePath);
    } catch {
      fail(`missing manifest file: ${entry.relativePath}`);
    }
    const actual = await sha256(entry.filePath);
    if (actual !== entry.expected) {
      fail(`hash mismatch: ${entry.relativePath}`);
    }
  }
  return { entries: entries.length };
}

if (process.argv[1] && path.resolve(process.argv[1]) === path.resolve(fileURLToPath(import.meta.url))) {
  const manifestPath = process.argv[2];
  if (!manifestPath) {
    console.error("usage: node scripts/verify-sha256-manifest.mjs <SHA256SUMS.txt>");
    process.exit(2);
  }
  try {
    const result = await verifyManifest(manifestPath);
    console.log(`verified ${result.entries} artifact hash${result.entries === 1 ? "" : "es"}`);
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
  }
}
