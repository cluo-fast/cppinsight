# Windows Release Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn the current Windows-oriented prototype into a reproducible, security-hardened, diagnosable Windows build that is ready for public-beta packaging.

**Architecture:** Keep the existing C++ scanner + Tauri desktop split. Add release verification around the bundle, isolate operating-system secret storage behind a Rust trait, expose a redacted diagnostic report through one Tauri command, and build the scanner and desktop together in Windows CI. Signing and updater credentials remain external secrets; the repository contains the complete wiring but never their private material.

**Tech Stack:** C++17/CMake/CTest, Rust 2021/Tauri 2/rusqlite/keyring, React 18/TypeScript/Vite, PowerShell, GitHub Actions

**Spec:** `docs/superpowers/specs/2026-08-27-public-beta-code-graph-design.md`

## Global Constraints

- The first public desktop release supports Windows only.
- Source code and indexes remain local unless the user explicitly invokes AI.
- The OpenAI API key must not be stored in SQLite or returned to the webview after saving.
- Tauri capabilities must stay least-privilege and CSP must be enabled.
- A failed or cancelled release-foundation operation must preserve existing project data.
- Generated diagnostics must exclude source contents, API keys, prompts, model answers, and absolute project paths.
- All committed release metadata must be reproducible by CI; private signing keys are GitHub secrets only.

---

## File Structure

### New files

- `desktop/src-tauri/src/secrets.rs` — OS credential-store abstraction and legacy SQLite migration.
- `desktop/src-tauri/src/diagnostics.rs` — redacted application diagnostic report construction and export.
- `desktop/src-tauri/icons/app-icon.svg` — editable source icon.
- `scripts/package-app.ps1` — deterministic local Windows build and bundle script.
- `scripts/verify-release-config.mjs` — fast structural validation of release files and Tauri metadata.
- `启动CppInsight.bat` — end-user/developer Windows launcher referenced by the documentation.
- `.github/workflows/windows-ci.yml` — C++, Rust, frontend, release-config, and unsigned bundle CI.
- `docs/releasing-windows.md` — signing, updater-secret, artifact, and release checklist.

### Modified files

- `desktop/src-tauri/src/lib.rs` — register focused modules and remove direct API-key persistence.
- `desktop/src-tauri/src/commands.rs` — expose safe settings and diagnostic commands.
- `desktop/src-tauri/Cargo.toml` / `Cargo.lock` — add OS credential-store dependency.
- `desktop/src-tauri/tauri.conf.json` — valid icons, CSP, Windows bundle metadata, updater artifact wiring.
- `desktop/src-tauri/capabilities/default.json` — explicit minimal permissions.
- `desktop/src/api.ts` / `desktop/src/types.ts` — redacted settings and diagnostic command types.
- `desktop/src/App.tsx` — safe API-key editor and diagnostic export action.
- `desktop/package.json` — release-config verification and test scripts.
- `README.md` / `desktop/README.md` — accurate product, build, privacy, and release instructions.
- `.gitignore` — generated signing and local release artifacts.

---

### Task 1: Make release metadata and bundle resources self-verifying

**Files:**
- Create: `scripts/verify-release-config.mjs`
- Create: `desktop/src-tauri/icons/app-icon.svg`
- Create: `scripts/package-app.ps1`
- Create: `启动CppInsight.bat`
- Modify: `desktop/src-tauri/tauri.conf.json`
- Modify: `desktop/package.json`
- Test: `scripts/verify-release-config.mjs`

**Interfaces:**
- Consumes: Tauri configuration at `desktop/src-tauri/tauri.conf.json`.
- Produces: `pnpm run verify:release`, a nonzero-exit validation command used locally and by CI.

- [ ] **Step 1: Write the failing release-config verifier**

Create a Node ESM script using only built-in modules. It must load `tauri.conf.json`, require the product metadata, verify that every configured icon and resource exists, reject suspicious icon names, and require both packaging launchers:

```js
import { existsSync, readFileSync } from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const tauriRoot = path.join(root, "desktop", "src-tauri");
const config = JSON.parse(readFileSync(path.join(tauriRoot, "tauri.conf.json"), "utf8"));
const failures = [];
const requireValue = (ok, message) => { if (!ok) failures.push(message); };

requireValue(config.productName === "CppInsight AI", "unexpected productName");
requireValue(config.identifier === "ai.cppinsight.desktop", "unexpected identifier");
requireValue(/^\d+\.\d+\.\d+$/.test(config.version), "version must be semver");
for (const relative of config.bundle?.icon ?? []) {
  requireValue(!relative.includes("@example."), `suspicious icon path: ${relative}`);
  requireValue(existsSync(path.join(tauriRoot, relative)), `missing icon: ${relative}`);
}
for (const relative of config.bundle?.resources ?? []) {
  requireValue(existsSync(path.join(tauriRoot, relative)), `missing resource: ${relative}`);
}
requireValue(existsSync(path.join(root, "scripts", "package-app.ps1")), "missing package script");
requireValue(existsSync(path.join(root, "启动CppInsight.bat")), "missing launcher");
if (failures.length) {
  console.error(failures.join("\n"));
  process.exit(1);
}
console.log("release configuration is structurally valid");
```

- [ ] **Step 2: Run the verifier to confirm current metadata fails**

Run:

```bash
node scripts/verify-release-config.mjs
```

Expected: FAIL listing the missing icons, scanner resource, `package-app.ps1`, and `启动CppInsight.bat`.

- [ ] **Step 3: Add the source icon and generate Tauri icons**

Create a square SVG with a dark navy background, a cyan graph of five nodes, and white `C++` lettering. Generate platform resources with:

```powershell
Set-Location desktop
pnpm tauri icon src-tauri/icons/app-icon.svg
```

Commit the generated `32x32.png`, `128x128.png`, `128x128@2x.png`, and `icon.ico`. Configure only these real paths.

- [ ] **Step 4: Add deterministic Windows launch and package scripts**

`scripts/package-app.ps1` must stop on errors, build the C++ scanner in Release, copy `cppinsight.exe` to `desktop/src-tauri/resources`, install locked frontend dependencies, run all verifiers, and invoke the Tauri build:

```powershell
$ErrorActionPreference = "Stop"
$RepoRoot = Split-Path -Parent $PSScriptRoot
cmake -S $RepoRoot -B "$RepoRoot/build-release" -DCMAKE_BUILD_TYPE=Release
cmake --build "$RepoRoot/build-release" --config Release
$Scanner = Join-Path $RepoRoot "build-release/Release/cppinsight.exe"
if (-not (Test-Path $Scanner)) { $Scanner = Join-Path $RepoRoot "build-release/cppinsight.exe" }
if (-not (Test-Path $Scanner)) { throw "cppinsight.exe was not produced" }
New-Item -ItemType Directory -Force "$RepoRoot/desktop/src-tauri/resources" | Out-Null
Copy-Item $Scanner "$RepoRoot/desktop/src-tauri/resources/cppinsight.exe" -Force
Push-Location "$RepoRoot/desktop"
try {
  pnpm install --frozen-lockfile
  pnpm run verify:release
  pnpm run build
  pnpm tauri build
} finally { Pop-Location }
```

The root launcher must invoke `scripts/start-desktop.ps1` relative to its own path and preserve the exit code.

- [ ] **Step 5: Make the verifier pass without weakening resource checks**

Because the bundled scanner is generated rather than committed, add an explicit `--allow-generated-scanner` argument used only before the packaging build has copied it. The packaging script itself must run the verifier after the copy without this flag.

Run:

```bash
node scripts/verify-release-config.mjs --allow-generated-scanner
```

Expected: PASS.

- [ ] **Step 6: Commit release metadata**

```bash
git add desktop/package.json desktop/src-tauri/tauri.conf.json desktop/src-tauri/icons scripts/package-app.ps1 scripts/verify-release-config.mjs 启动CppInsight.bat
git commit -m "build: add reproducible Windows packaging foundation"
```

---

### Task 2: Move the AI API key to the operating-system credential store

**Files:**
- Create: `desktop/src-tauri/src/secrets.rs`
- Modify: `desktop/src-tauri/src/lib.rs`
- Modify: `desktop/src-tauri/src/commands.rs`
- Modify: `desktop/src-tauri/Cargo.toml`
- Modify: `desktop/src/types.ts`
- Modify: `desktop/src/api.ts`
- Modify: `desktop/src/App.tsx`
- Test: unit tests in `desktop/src-tauri/src/secrets.rs`

**Interfaces:**
- Produces: `SecretBackend::{get,set,delete}`; `load_ai_settings(connection, secrets) -> AiRuntimeSettings`; `migrate_legacy_api_key(connection, secrets) -> Result<bool, String>`.
- Produces to frontend: `PublicAiSettings { base_url, model, api_key_configured }` and `UpdateAiSettings { base_url, model, api_key: Option<String>, clear_api_key: bool }`.
- Consumes: the existing SQLite `settings` table for non-secret base URL and model values.

- [ ] **Step 1: Add failing secret migration tests**

Define an in-memory backend and tests proving that migration copies the legacy value first, removes the SQLite row only after a successful secret write, and never exposes the secret through public settings:

```rust
#[test]
fn migrates_legacy_key_then_deletes_plaintext() {
    let db = Connection::open_in_memory().unwrap();
    migrate_settings_table(&db).unwrap();
    upsert_setting_value(&db, "openai_api_key", "sk-legacy").unwrap();
    let secrets = MemorySecretBackend::default();

    assert!(migrate_legacy_api_key(&db, &secrets).unwrap());
    assert_eq!(secrets.get(OPENAI_ACCOUNT).unwrap().as_deref(), Some("sk-legacy"));
    assert_eq!(read_setting_value(&db, "openai_api_key").unwrap(), None);
}

#[test]
fn failed_secret_write_preserves_legacy_key() {
    let db = legacy_settings_db("sk-preserve");
    let secrets = FailingSecretBackend;
    assert!(migrate_legacy_api_key(&db, &secrets).is_err());
    assert_eq!(read_setting_value(&db, "openai_api_key").unwrap().as_deref(), Some("sk-preserve"));
}
```

- [ ] **Step 2: Run the Rust tests and confirm they fail**

Run:

```bash
cargo test secrets --manifest-path desktop/src-tauri/Cargo.toml
```

Expected: FAIL because `secrets` and its interfaces do not exist.

- [ ] **Step 3: Implement the backend and migration**

Add `keyring = { version = "4.1", features = ["v1"] }`. Implement the production adapter using `keyring::v1::Entry` with service `ai.cppinsight.desktop` and account `openai_api_key`. Treat `NoEntry` as `None`; preserve other errors.

```rust
pub trait SecretBackend: Send + Sync {
    fn get(&self, account: &str) -> Result<Option<String>, String>;
    fn set(&self, account: &str, value: &str) -> Result<(), String>;
    fn delete(&self, account: &str) -> Result<(), String>;
}

pub const OPENAI_ACCOUNT: &str = "openai_api_key";
pub const SERVICE_NAME: &str = "ai.cppinsight.desktop";
```

Environment variable `OPENAI_API_KEY` remains the highest-priority runtime override and is never copied into the credential store.

- [ ] **Step 4: Replace the frontend settings contract**

The get command returns only `api_key_configured`. Saving with an empty `api_key` preserves the stored value unless `clear_api_key` is true. The password field starts empty with placeholder `已安全保存；留空保持不变`.

```ts
export interface PublicAiSettings {
  base_url: string;
  model: string;
  api_key_configured: boolean;
}

export interface UpdateAiSettings {
  base_url: string;
  model: string;
  api_key: string | null;
  clear_api_key: boolean;
}
```

- [ ] **Step 5: Run targeted and full tests**

Run:

```bash
cargo test --manifest-path desktop/src-tauri/Cargo.toml
pnpm --dir desktop run build
rg -n 'openai_api_key.*value|api_key TEXT|upsert_setting_value\([^\n]*openai_api_key' desktop/src-tauri/src
```

Expected: tests/build PASS; the plaintext persistence scan has no matches outside the explicit migration code and tests.

- [ ] **Step 6: Commit secure settings**

```bash
git add desktop/src-tauri/src/secrets.rs desktop/src-tauri/src/lib.rs desktop/src-tauri/src/commands.rs desktop/src-tauri/Cargo.toml desktop/src-tauri/Cargo.lock desktop/src/types.ts desktop/src/api.ts desktop/src/App.tsx
git commit -m "security: store AI credentials in the OS keyring"
```

---

### Task 3: Enable a strict CSP and verify least-privilege capabilities

**Files:**
- Modify: `desktop/src-tauri/tauri.conf.json`
- Modify: `desktop/src-tauri/capabilities/default.json`
- Modify: `scripts/verify-release-config.mjs`
- Test: `scripts/verify-release-config.mjs`

**Interfaces:**
- Consumes: all current frontend asset and IPC requirements.
- Produces: a CSP that permits bundled assets and Tauri IPC but no remote scripts or arbitrary navigation.

- [ ] **Step 1: Extend the verifier with failing security assertions**

Require a non-null CSP and an exact permission allowlist:

```js
const csp = config.app?.security?.csp;
requireValue(typeof csp === "string" && csp.includes("default-src 'self'"), "CSP must default to self");
requireValue(csp.includes("connect-src ipc: http://ipc.localhost"), "CSP must allow Tauri IPC");
requireValue(!csp.includes("https:"), "CSP must not allow arbitrary HTTPS from the webview");
```

Parse `capabilities/default.json` and fail if permissions differ from `core:default`, `core:event:default`, and `dialog:default`.

- [ ] **Step 2: Run the verifier and confirm CSP failure**

Run `node scripts/verify-release-config.mjs --allow-generated-scanner`.

Expected: FAIL with `CSP must default to self`.

- [ ] **Step 3: Configure CSP and retain Rust-owned network calls**

Set:

```json
"csp": "default-src 'self' customprotocol: asset:; connect-src ipc: http://ipc.localhost; img-src 'self' asset: http://asset.localhost blob: data:; style-src 'self' 'unsafe-inline'; font-src 'self' data:; object-src 'none'; frame-src 'none'; base-uri 'none'; form-action 'none'"
```

The frontend must continue invoking Rust for model requests; do not add OpenAI or proxy domains to `connect-src`.

- [ ] **Step 4: Run verification and application builds**

```bash
node scripts/verify-release-config.mjs --allow-generated-scanner
pnpm --dir desktop run build
cargo test --manifest-path desktop/src-tauri/Cargo.toml
```

Expected: PASS.

- [ ] **Step 5: Commit security policy**

```bash
git add desktop/src-tauri/tauri.conf.json desktop/src-tauri/capabilities/default.json scripts/verify-release-config.mjs
git commit -m "security: enforce desktop CSP and capability allowlist"
```

---

### Task 4: Add a redacted, user-previewable diagnostic export

**Files:**
- Create: `desktop/src-tauri/src/diagnostics.rs`
- Modify: `desktop/src-tauri/src/lib.rs`
- Modify: `desktop/src-tauri/src/commands.rs`
- Modify: `desktop/src/types.ts`
- Modify: `desktop/src/api.ts`
- Modify: `desktop/src/App.tsx`
- Test: unit tests in `desktop/src-tauri/src/diagnostics.rs`

**Interfaces:**
- Produces: `DiagnosticReport`, `build_diagnostic_report(app_data_dir, workspace, projects)`, and Tauri command `export_diagnostics(path) -> String`.
- Guarantees: the serialized report contains only app/build versions, OS/architecture, index counts, normalized error categories, and hashed project identifiers.

- [ ] **Step 1: Write failing redaction tests**

```rust
#[test]
fn diagnostic_json_omits_paths_source_and_secrets() {
    let report = build_fixture_report(
        r"C:\Users\Alice\client-secret",
        "sk-super-secret",
        "int main() { return 0; }",
    );
    let json = serde_json::to_string(&report).unwrap();
    assert!(!json.contains("Alice"));
    assert!(!json.contains("client-secret"));
    assert!(!json.contains("sk-super-secret"));
    assert!(!json.contains("int main"));
    assert!(json.contains("project_hash"));
}
```

- [ ] **Step 2: Run the test and confirm failure**

Run `cargo test diagnostics --manifest-path desktop/src-tauri/Cargo.toml`.

Expected: FAIL because the module does not exist.

- [ ] **Step 3: Implement deterministic redaction and export**

Use a deterministic non-cryptographic identifier only for correlating repeated local reports; do not serialize its input. Include a visible `redaction` section describing omitted fields. Write pretty JSON to the user-selected destination via a temp file and rename.

```rust
#[derive(Debug, Serialize)]
pub struct DiagnosticProject {
    pub project_hash: String,
    pub file_count: u64,
    pub symbol_count: u64,
    pub index_mode: String,
    pub diagnostic_categories: Vec<String>,
}
```

- [ ] **Step 4: Add preview and export to Settings**

The frontend first invokes `preview_diagnostics`, renders the JSON in a scrollable `<pre>`, then enables `导出诊断信息`. File selection uses the existing dialog plugin. No automatic upload is added.

- [ ] **Step 5: Run tests and builds**

```bash
cargo test --manifest-path desktop/src-tauri/Cargo.toml
pnpm --dir desktop run build
```

Expected: PASS.

- [ ] **Step 6: Commit diagnostics**

```bash
git add desktop/src-tauri/src/diagnostics.rs desktop/src-tauri/src/lib.rs desktop/src-tauri/src/commands.rs desktop/src/types.ts desktop/src/api.ts desktop/src/App.tsx
git commit -m "feat: export redacted desktop diagnostics"
```

---

### Task 5: Add Windows CI and signing-ready release wiring

**Files:**
- Create: `.github/workflows/windows-ci.yml`
- Create: `docs/releasing-windows.md`
- Modify: `.gitignore`
- Modify: `scripts/verify-release-config.mjs`
- Test: workflow syntax and release verifier

**Interfaces:**
- Produces: PR CI with C++/Rust/frontend tests and an unsigned NSIS/MSI bundle artifact.
- Consumes for signed releases: `WINDOWS_CERTIFICATE`, `WINDOWS_CERTIFICATE_PASSWORD`, `TAURI_SIGNING_PRIVATE_KEY`, and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` GitHub secrets.

- [ ] **Step 1: Add failing workflow checks to the verifier**

Require `.github/workflows/windows-ci.yml` to contain `windows-latest`, `ctest`, `cargo test`, `pnpm run build`, `pnpm tauri build`, and artifact upload.

- [ ] **Step 2: Run the verifier and confirm the workflow is missing**

Run `node scripts/verify-release-config.mjs --allow-generated-scanner`.

Expected: FAIL with `missing Windows CI workflow`.

- [ ] **Step 3: Implement PR and manual-release jobs**

The workflow must:

1. Check out the repository.
2. Install stable Rust with MSVC target and pnpm.
3. Configure/build/test C++ Release.
4. Copy `cppinsight.exe` into Tauri resources.
5. Install frontend dependencies with the lockfile.
6. Run release verifier, TypeScript/Vite build, and Cargo tests.
7. Build NSIS/MSI bundles.
8. Upload installers and SHA-256 files.
9. Import and use a Windows signing certificate only when manual release inputs and required secrets are present.
10. Never print certificate or updater private-key values.

- [ ] **Step 4: Document external release prerequisites explicitly**

`docs/releasing-windows.md` must state that the repository cannot manufacture publisher identity. It must document certificate acquisition, GitHub secret names, local unsigned builds, signed release invocation, checksum verification, rollback, and how updater signing differs from Authenticode signing.

- [ ] **Step 5: Validate configuration locally**

```bash
node scripts/verify-release-config.mjs --allow-generated-scanner
git diff --check
```

Expected: PASS. The first pushed workflow run is the authoritative Windows bundle verification.

- [ ] **Step 6: Commit CI and release documentation**

```bash
git add .github/workflows/windows-ci.yml docs/releasing-windows.md .gitignore scripts/verify-release-config.mjs
git commit -m "ci: build and verify Windows desktop bundles"
```

---

### Task 6: Replace stale product documentation and run the release-foundation gate

**Files:**
- Modify: `README.md`
- Modify: `desktop/README.md`
- Modify: `desktop/package.json`
- Test: all project verification commands

**Interfaces:**
- Produces: one accurate entry point for users and one developer/release guide.

- [ ] **Step 1: Add documentation checks to the verifier**

Require the root README to mention `项目接手报告`, `关系图`, `MCP`, `隐私`, and the Windows launcher. Require the desktop README to reference `scripts/package-app.ps1` and `docs/releasing-windows.md`.

- [ ] **Step 2: Confirm the stale README fails verification**

Run `node scripts/verify-release-config.mjs --allow-generated-scanner`.

Expected: FAIL listing missing product documentation markers.

- [ ] **Step 3: Rewrite user and developer documentation**

Document current implemented capabilities separately from the approved roadmap. Do not claim the five graph views are implemented until their later phase lands. Include local-only behavior, optional AI context transmission, current heuristic limitations, CLI/MCP usage, Windows development, packaging, tests, and feedback diagnostics.

- [ ] **Step 4: Run the full release-foundation gate**

```bash
cmake -S . -B /tmp/cppinsight-release-foundation -DBUILD_TESTING=ON
cmake --build /tmp/cppinsight-release-foundation -j2
ctest --test-dir /tmp/cppinsight-release-foundation --output-on-failure
node scripts/verify-release-config.mjs --allow-generated-scanner
pnpm --dir desktop run build
cargo test --manifest-path desktop/src-tauri/Cargo.toml
git diff --check
```

Expected: all commands PASS. On macOS, packaging itself is not claimed; Windows CI is the authoritative NSIS/MSI test.

- [ ] **Step 5: Commit documentation and close the phase**

```bash
git add README.md desktop/README.md desktop/package.json
git commit -m "docs: document public beta release foundation"
```

- [ ] **Step 6: Record evidence**

Append actual command results and the Windows CI run URL to the phase handoff. If the CI run has not occurred because changes are not pushed, record that exact external-state dependency without marking Windows packaging or signing complete.

