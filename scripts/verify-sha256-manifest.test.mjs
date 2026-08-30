import assert from "node:assert/strict";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { createHash } from "node:crypto";

import { manifestPathKey, verifyManifest } from "./verify-sha256-manifest.mjs";

async function withFixture(run) {
  const root = await mkdtemp(path.join(os.tmpdir(), "cppinsight-sha256-"));
  try {
    return await run(root);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
}

function digest(content) {
  return createHash("sha256").update(content).digest("hex");
}

test("SHA-256 manifest verifies listed files and Windows separators", async () => {
  await withFixture(async (root) => {
    const nested = path.join(root, "bundle", "CppInsight-AI-setup.exe");
    await mkdir(path.dirname(nested), { recursive: true });
    await writeFile(nested, "installer-bytes");
    const manifest = path.join(root, "bundle", "SHA256SUMS.txt");
    await writeFile(manifest, `${digest("installer-bytes")}  CppInsight-AI-setup.exe\n`);

    const result = await verifyManifest(manifest);
    assert.deepEqual(result, { entries: 1 });

    const windowsManifest = path.join(root, "windows-SHA256SUMS.txt");
    await writeFile(windowsManifest, `${digest("installer-bytes")}  bundle\\CppInsight-AI-setup.exe\n`);
    assert.deepEqual(await verifyManifest(windowsManifest), { entries: 1 });
    assert.equal((await readFile(nested)).toString(), "installer-bytes");
  });
});

test("SHA-256 manifest rejects mismatches, duplicate entries, and unsafe paths", async () => {
  await withFixture(async (root) => {
    const file = path.join(root, "artifact.msi");
    await writeFile(file, "actual");
    const manifest = path.join(root, "SHA256SUMS.txt");

    await writeFile(manifest, `${digest("expected")}  artifact.msi\n`);
    await assert.rejects(() => verifyManifest(manifest), /hash mismatch/);

    await writeFile(manifest, `${digest("actual")}  artifact.msi\n${digest("actual")}  artifact.msi\n`);
    await assert.rejects(() => verifyManifest(manifest), /duplicate manifest entry/);

    await writeFile(manifest, `${digest("actual")}  ..\\outside.msi\n`);
    await assert.rejects(() => verifyManifest(manifest), /unsafe manifest path/);

    await writeFile(manifest, `${digest("actual")}  C:\\\\outside.msi\n`);
    await assert.rejects(() => verifyManifest(manifest), /unsafe manifest path/);
  });
});

test("manifest duplicate keys can model Windows case-insensitive paths", () => {
  assert.equal(
    manifestPathKey("C:/bundle/CppInsight-AI.msi", true),
    manifestPathKey("c:/BUNDLE/cppinsight-ai.msi", true)
  );
  assert.notEqual(
    manifestPathKey("/tmp/Bundle/CppInsight-AI.msi", false),
    manifestPathKey("/tmp/bundle/cppinsight-ai.msi", false)
  );
});
