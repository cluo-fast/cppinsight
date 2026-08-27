import { existsSync, readFileSync } from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const tauriRoot = path.join(root, "desktop", "src-tauri");
const allowGeneratedScanner = process.argv.includes("--allow-generated-scanner");
const failures = [];

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

requireValue(config.productName === "CppInsight AI", "unexpected productName");
requireValue(config.identifier === "ai.cppinsight.desktop", "unexpected identifier");
requireValue(/^\d+\.\d+\.\d+$/.test(config.version), "version must be semver");

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
requireFile(path.join(root, "启动CppInsight.bat"), "Windows launcher");

if (failures.length > 0) {
  console.error(failures.join("\n"));
  process.exit(1);
}

console.log("release configuration is structurally valid");
