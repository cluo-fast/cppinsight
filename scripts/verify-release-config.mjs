import { existsSync, readFileSync } from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const tauriRoot = path.join(root, "desktop", "src-tauri");
const allowGeneratedScanner = process.argv.includes("--allow-generated-scanner");
const failures = [];
const expectedCsp = "default-src 'self' customprotocol: asset:; connect-src ipc: http://ipc.localhost; img-src 'self' asset: http://asset.localhost blob: data:; style-src 'self' 'unsafe-inline'; font-src 'self' data:; object-src 'none'; frame-src 'none'; base-uri 'none'; form-action 'none'";

function requireValue(condition, message) {
  if (!condition) {
    failures.push(message);
  }
}

function requireFile(filePath, label) {
  requireValue(existsSync(filePath), `missing ${label}: ${path.relative(root, filePath)}`);
}

const configPath = path.join(tauriRoot, "tauri.conf.json");
const config = JSON.parse(readFileSync(configPath, "utf8"));
const desktopPackage = JSON.parse(
  readFileSync(path.join(root, "desktop", "package.json"), "utf8")
);

requireValue(config.productName === "CppInsight AI", "unexpected productName");
requireValue(config.identifier === "ai.cppinsight.desktop", "unexpected identifier");
requireValue(/^\d+\.\d+\.\d+$/.test(config.version), "version must be semver");
requireValue(
  desktopPackage.scripts?.["verify:bundle"] ===
    "node ../scripts/verify-release-config.mjs",
  "desktop package must provide strict verify:bundle script"
);
requireValue(
  desktopPackage.scripts?.["test:graph-snapshot"] ===
    "node --experimental-strip-types ../scripts/graph-snapshot.test.mjs",
  "desktop package must provide graph snapshot regression test"
);
requireValue(
  desktopPackage.scripts?.["test:benchmark-result"] ===
    "node --test ../scripts/verify-benchmark-result.test.mjs",
  "desktop package must provide benchmark result validation test"
);
requireValue(
  desktopPackage.scripts?.["test:benchmark-corpus"] ===
    "node --test ../scripts/verify-benchmark-corpus.test.mjs",
  "desktop package must provide benchmark corpus validation test"
);
requireValue(config.build?.beforeDevCommand === "pnpm run dev", "beforeDevCommand must use pnpm");
requireValue(config.build?.beforeBuildCommand === "pnpm run build", "beforeBuildCommand must use pnpm");
requireValue(
  config.app?.security?.csp === expectedCsp,
  "desktop CSP must match the reviewed strict policy"
);

const capabilityPath = path.join(tauriRoot, "capabilities", "default.json");
const capability = JSON.parse(readFileSync(capabilityPath, "utf8"));
const expectedPermissions = ["core:default", "core:event:default", "dialog:default"];
requireValue(
  JSON.stringify(capability.permissions) === JSON.stringify(expectedPermissions),
  `desktop permissions must match the reviewed allowlist: ${expectedPermissions.join(", ")}`
);

for (const relative of config.bundle?.icon ?? []) {
  requireValue(!relative.includes("@example."), `suspicious icon path: ${relative}`);
  requireFile(path.join(tauriRoot, relative), "icon");
}

for (const relative of config.bundle?.resources ?? []) {
  const isGeneratedScanner = relative.replaceAll("\\", "/") === "resources/cppinsight.exe";
  if (allowGeneratedScanner && isGeneratedScanner) {
    continue;
  }
  requireFile(path.join(tauriRoot, relative), "resource");
}

requireFile(path.join(root, "scripts", "package-app.ps1"), "package script");
requireFile(path.join(root, "scripts", "windows-installer-smoke.ps1"), "Windows installer smoke script");
requireFile(path.join(root, "启动CppInsight.bat"), "Windows launcher");

const windowsWorkflowPath = path.join(root, ".github", "workflows", "windows-ci.yml");
if (existsSync(windowsWorkflowPath)) {
  const workflow = readFileSync(windowsWorkflowPath, "utf8");
  for (const marker of [
    "windows-latest",
    "ctest",
    "cargo test",
    "pnpm run build",
    "pnpm run test:graph-snapshot",
    "pnpm run test:benchmark-result",
    "pnpm run test:benchmark-corpus",
    "pnpm tauri build",
    "Verify bundled sample scan and graph query",
    "examples/sample-cpp",
    "cppinsight-workbench.exe index",
    "Upload Windows installers",
    "Smoke test MSI and NSIS installers",
    "windows-installer-smoke.ps1",
    "Get-FileHash",
    "Get-AuthenticodeSignature"
  ]) {
    requireValue(workflow.includes(marker), `Windows CI workflow is missing: ${marker}`);
  }
  requireValue(
    !workflow.includes("TAURI_SIGNING_PRIVATE_KEY"),
    "Windows CI must not expose updater signing secrets while the updater is disabled"
  );
  for (const line of workflow.split(/\r?\n/)) {
    const action = line.match(/^\s*uses:\s*([^\s#]+)(?:\s*#.*)?$/)?.[1];
    if (action) {
      requireValue(
        /@[0-9a-f]{40}$/.test(action),
        `GitHub Action must be pinned to a full commit SHA: ${action}`
      );
    }
  }
} else {
  failures.push("missing Windows CI workflow: .github/workflows/windows-ci.yml");
}

function requireTextMarkers(filePath, label, markers) {
  if (!existsSync(filePath)) {
    failures.push(`missing ${label}: ${path.relative(root, filePath)}`);
    return;
  }
  const content = readFileSync(filePath, "utf8");
  for (const marker of markers) {
    requireValue(content.includes(marker), `${label} is missing: ${marker}`);
  }
}

requireTextMarkers(path.join(root, "README.md"), "root README", [
  "项目接手报告",
  "关系图",
  "MCP",
  "隐私",
  "启动CppInsight.bat",
  "体验内置示例"
]);
requireTextMarkers(path.join(root, "desktop", "README.md"), "desktop README", [
  "scripts/package-app.ps1",
  "docs/releasing-windows.md",
  "体验内置示例"
]);
requireTextMarkers(path.join(root, "desktop", "src", "App.tsx"), "desktop app shell", [
  "提交反馈",
  "settings-privacy-note",
  "getVersion",
  "未找到 compile_commands.json",
  "requestedGraphNodeId",
  "在函数调用图中查看"
]);
requireTextMarkers(path.join(root, "desktop", "src", "GraphWorkbench.tsx"), "graph workbench navigation", [
  "requestedGraphNodeId",
  "setRootIds([requestedGraphNodeId])",
  "setRootIds([])"
]);
requireTextMarkers(path.join(root, "desktop", "src-tauri", "src", "lib.rs"), "client summary generator", [
  "## Suggested reading order",
  "Graph findings are heuristic",
  "stable_id: row.get(0)?"
]);
requireTextMarkers(path.join(root, ".github", "ISSUE_TEMPLATE", "bug_report.yml"), "public beta bug form", [
  "应用版本",
  "复现步骤",
  "脱敏诊断 JSON",
  "API Key",
  "privacy"
]);
requireTextMarkers(path.join(root, "scripts", "benchmark-graph.ps1"), "graph benchmark script", [
  "RedirectStandardInput",
  "peak_working_set_mb",
  "Get-Median",
  "project_name"
]);
requireFile(path.join(root, "scripts", "graph-snapshot.test.mjs"), "graph snapshot regression test");
requireFile(path.join(root, "desktop", "src", "graphSnapshot.ts"), "graph snapshot renderer");
requireFile(path.join(root, "scripts", "verify-benchmark-result.mjs"), "benchmark result verifier");
requireFile(path.join(root, "scripts", "verify-benchmark-result.test.mjs"), "benchmark result verifier test");
requireFile(path.join(root, "docs", "benchmarks", "corpus.json"), "benchmark corpus manifest");
requireFile(path.join(root, "scripts", "verify-benchmark-corpus.mjs"), "benchmark corpus verifier");
requireFile(path.join(root, "scripts", "verify-benchmark-corpus.test.mjs"), "benchmark corpus verifier test");

if (failures.length > 0) {
  console.error(failures.join("\n"));
  process.exit(1);
}

console.log("release configuration is structurally valid");
