# Release Foundation Handoff

Date: 2026-08-28
Branch: `codex/release-foundation`

## Completed scope

- Reproducible pnpm lockfile, Tauri icons, scanner resource wiring, Windows launcher and packaging script.
- OS credential-store API key persistence with safe legacy SQLite migration.
- Strict desktop CSP and reviewed Tauri permission allowlist.
- Redacted diagnostic preview/export with anonymous project IDs and no-overwrite behavior.
- Windows GitHub Actions workflow for scanner/frontend/Rust tests, NSIS/MSI bundles, SHA-256 manifest, unsigned PR artifacts and optional manual Authenticode signing.
- Product, privacy, development and Windows release documentation.

## Local verification evidence

The release-foundation gate passed on macOS arm64:

- `cmake -S . -B /tmp/cppinsight-release-foundation -DBUILD_TESTING=ON`
- `cmake --build /tmp/cppinsight-release-foundation -j2`
- `ctest --test-dir /tmp/cppinsight-release-foundation --output-on-failure`: 1/1 passed.
- `pnpm --dir desktop run verify:release`: passed.
- `pnpm --dir desktop run build`: TypeScript and Vite production build passed.
- `cargo test --manifest-path desktop/src-tauri/Cargo.toml`: 32/32 library tests passed; binary and doc-test targets passed.
- `git diff --check`: passed.
- Local browser layout check at 1280×720: settings/diagnostic controls rendered without horizontal overflow.

The Rust build retains one pre-existing dead-code warning for `enclosing_definition`; it does not fail tests.

An independent code review was completed before branch handoff. Its blocking and important findings were addressed by removing updater secrets from PR builds, pinning Actions to immutable commits, verifying Authenticode certificate fingerprints, retrying legacy-key migration during workspace restore/settings/AI use, preserving AppData-first precedence when old credential sources conflict, allowlisting every serialized diagnostic string, rejecting conflicting key updates, and making the CSP check exact. Diagnostic export now uses a temporary file plus atomic no-clobber persistence, and export remains disabled until the user previews the report.

## External verification still required

No Windows CI run URL exists yet because this local branch has not been pushed. Consequently:

- Windows-native NSIS/MSI production is **not yet proven**.
- Installer launch/install/uninstall behavior is **not yet proven**.
- Authenticode signing is wired but **not verified** because no publisher certificate or GitHub secrets were supplied.
- Tauri updater signing secrets are documented but automatic updating is intentionally not enabled.

After the branch is published, record the first successful `.github/workflows/windows-ci.yml` run URL, commit SHA and installer hashes here. Do not mark Windows packaging or signing complete before that evidence exists.

## Next implementation phase

Proceed to the versioned graph data/query foundation shared by desktop, CLI and MCP, then implement the five required Cytoscape.js graph views. The approved requirements remain in `docs/superpowers/specs/2026-08-27-public-beta-code-graph-design.md`.
