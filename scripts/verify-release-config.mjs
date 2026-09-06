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
  desktopPackage.scripts?.["test:graph-workbench-state"] ===
    "node --experimental-strip-types ../scripts/graph-workbench-state.test.mjs",
  "desktop package must provide GraphWorkbench state regression test"
);
requireValue(
  desktopPackage.scripts?.["test:report-html"] ===
    "node --experimental-strip-types ../scripts/report-html.test.mjs",
  "desktop package must provide HTML report security regression test"
);
requireValue(
  desktopPackage.scripts?.["test:runtime-mode"] ===
    "node --experimental-strip-types ../scripts/runtime-mode.test.mjs",
  "desktop package must provide runtime mode regression test"
);
requireValue(
  desktopPackage.scripts?.["test:ai-mode"] ===
    "node --experimental-strip-types ../scripts/ai-mode.test.mjs",
  "desktop package must provide AI mode regression test"
);
requireValue(
  desktopPackage.scripts?.["test:artifact-manifest"] ===
    "node --test ../scripts/verify-sha256-manifest.test.mjs",
  "desktop package must provide artifact manifest verification test"
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
requireTextMarkers(path.join(root, "scripts", "package-app.ps1"), "package script", [
  "Get-FileHash",
  "SHA256SUMS.txt",
  "verify-sha256-manifest.mjs",
  "pnpm run check",
  "--bin cppinsight-workbench"
]);
requireFile(path.join(root, "scripts", "windows-installer-smoke.ps1"), "Windows installer smoke script");
requireTextMarkers(path.join(root, "scripts", "windows-installer-smoke.ps1"), "Windows installer smoke script", [
  "Assert-AppStarts",
  "WaitForExit(5000)",
  "MSI reinstall",
  "NSIS reinstall"
]);
requireFile(path.join(root, "启动CppInsight.bat"), "Windows launcher");
requireFile(
  path.join(root, "examples", "sample-cpp", "compile_commands.json"),
  "bundled sample compile database"
);

const windowsWorkflowPath = path.join(root, ".github", "workflows", "windows-ci.yml");
if (existsSync(windowsWorkflowPath)) {
  const workflow = readFileSync(windowsWorkflowPath, "utf8");
  for (const marker of [
    "windows-latest",
    "branches: [main, 'codex/**']",
    "ctest",
    "cargo test",
    "pnpm run build",
    "pnpm run test:graph-snapshot",
    "pnpm run test:graph-workbench-state",
    "pnpm run test:report-html",
    "pnpm run test:runtime-mode",
    "pnpm run test:ai-mode",
    "pnpm run test:artifact-manifest",
    "pnpm run test:benchmark-result",
    "pnpm run test:benchmark-corpus",
    "pnpm tauri build",
    "Verify bundled sample scan and graph query",
    "build-release/cppinsight.exe",
    "cppinsight.exe was not produced in build-release",
    "examples/sample-cpp",
    "module_dependencies",
    "function_calls",
    "class_relations",
    "file_includes",
    "entry_paths",
    "cppinsight-workbench.exe index",
    "Build release Workbench CLI",
    "cargo build --manifest-path desktop/src-tauri/Cargo.toml --release --bin cppinsight-workbench",
    "Upload Windows installers",
    "Smoke test MSI and NSIS installers",
    "windows-installer-smoke.ps1",
    "Get-FileHash",
    "verify-sha256-manifest.mjs",
    "Get-AuthenticodeSignature",
    "TimeStamperCertificate"
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
requireTextMarkers(path.join(root, "docs", "licensing.md"), "license boundary document", [
  "open-core",
  "Apache-2.0",
  "desktop/",
  "第三方声明"
]);
requireFile(path.join(root, "docs", "release-notes-template.md"), "release notes template");
requireTextMarkers(path.join(root, "docs", "release-notes-template.md"), "release notes template", [
  "源码提交 SHA",
  "性能实测",
  "已知限制与解析器边界",
  "隐私与安全",
  "SHA-256",
  "回滚版本"
]);
requireTextMarkers(path.join(root, "desktop", "src", "App.tsx"), "desktop app shell", [
  "提交反馈",
  "settings-privacy-note",
  "previewAskContext",
  "getVersion",
  "未找到 compile_commands.json",
  "requestedGraphNodeId",
  "graphRequestId",
  "graphDataVersion",
  "取消索引",
  "summaryRequestSequenceRef",
  "exportClientSummary(active.overview.root",
  "正在生成本地接手报告",
  "完成索引后将生成本地接手报告",
  "ai_enabled",
  "本地检索",
  "在函数调用图中查看",
  "graph_node_id",
  "回到关系图"
]);
requireTextMarkers(path.join(root, "desktop", "src", "aiMode.ts"), "AI local-only mode helper", [
  "aiModeLabel",
  "canSaveAiSettings",
  "askContextDescription",
  "发送前确认上下文",
  "本地检索结果",
  "AI 已关闭，仅本地分析",
  "只发送下列文件和行号范围"
]);
requireTextMarkers(path.join(root, "desktop", "src", "GraphWorkbench.tsx"), "graph workbench navigation", [
  "requestedGraphNodeId",
  "graphRequestId",
  "graphDataVersion",
  "graphQuerySequenceRef",
  "changeGraphKind",
  "清除查询根",
  "清除路径目标",
  "setRootIds([requestedGraphNodeId])",
  "setRootIds([])",
  "按页加载",
  "response.groups.length"
]);
requireTextMarkers(path.join(root, "desktop", "src", "graphWorkbenchState.ts"), "GraphWorkbench scope state", [
  "reconcileGraphScope"
]);
requireTextMarkers(path.join(root, "desktop", "src-tauri", "src", "lib.rs"), "client summary generator", [
  "## Build context",
  "## First 1-3 days",
  "## Suggested reading order",
  "Graph findings are heuristic",
  "stable_id: row.get(0)?"
]);
requireTextMarkers(path.join(root, "desktop", "src-tauri", "src", "commands.rs"), "AI context preview command", [
  "pub async fn preview_ask_context",
  "build_ask_context_preview"
]);
requireTextMarkers(path.join(root, "desktop", "src-tauri", "src", "commands.rs"), "index cancellation command", [
  "IndexCancellation",
  "cancel_index",
  "indexing cancelled",
  "load_project_overview_scoped"
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
requireTextMarkers(path.join(root, "scripts", "graph-snapshot.test.mjs"), "graph snapshot coverage", [
  "covers every first-release graph kind"
]);
requireFile(path.join(root, "desktop", "src", "graphSnapshot.ts"), "graph snapshot renderer");
requireFile(path.join(root, "scripts", "graph-workbench-state.test.mjs"), "GraphWorkbench state regression test");
requireFile(path.join(root, "desktop", "src", "graphWorkbenchState.ts"), "GraphWorkbench state helpers");
requireFile(path.join(root, "scripts", "report-html.test.mjs"), "HTML report security regression test");
requireFile(path.join(root, "desktop", "src", "reportHtml.ts"), "HTML report renderer");
requireFile(path.join(root, "scripts", "runtime-mode.test.mjs"), "runtime mode regression test");
requireFile(path.join(root, "desktop", "src", "runtime.ts"), "runtime mode helper");
requireFile(path.join(root, "scripts", "verify-sha256-manifest.mjs"), "artifact hash manifest verifier");
requireFile(path.join(root, "scripts", "verify-sha256-manifest.test.mjs"), "artifact hash manifest verifier test");
requireFile(path.join(root, "scripts", "verify-benchmark-result.mjs"), "benchmark result verifier");
requireFile(path.join(root, "scripts", "verify-benchmark-result.test.mjs"), "benchmark result verifier test");
requireFile(path.join(root, "docs", "benchmarks", "corpus.json"), "benchmark corpus manifest");
requireFile(path.join(root, "scripts", "verify-benchmark-corpus.mjs"), "benchmark corpus verifier");
requireFile(path.join(root, "scripts", "verify-benchmark-corpus.test.mjs"), "benchmark corpus verifier test");
requireFile(
  path.join(root, "tools", "mcp-server", "test", "transport.test.mjs"),
  "MCP SDK stdio transport test"
);

if (failures.length > 0) {
  console.error(failures.join("\n"));
  process.exit(1);
}

console.log("release configuration is structurally valid");
