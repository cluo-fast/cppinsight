# CppInsight AI

[简体中文](README.zh-CN.md)

[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
[![Windows CI](https://github.com/cluo-fast/cppinsight/actions/workflows/windows-ci.yml/badge.svg)](https://github.com/cluo-fast/cppinsight/actions/workflows/windows-ci.yml)

**Local onboarding for an unfamiliar or legacy C/C++ codebase: a heuristic scan, a SQLite index, five relationship graphs, and optional AI that can stay entirely on your machine.**

CppInsight AI helps you take over a C/C++ tree you did not write. It inventories the source, writes a project-local index, and produces an onboarding report plus relationship graphs you can check against the cited lines. It is a desktop workbench, a small CLI, and an MCP server. It is not a compiler, and it is not a general chat client.

## Not C++ Insights

[C++ Insights](https://cppinsights.io) (Andreas Fertig) shows how a compiler front end transforms modern C++: templates, lambdas, range-for, and similar rewrites, using Clang. **CppInsight AI does not do that.** It does not compile your project and it does not print compiler transformations. The names are similar; the tools are not. If you want to see what the compiler does to a snippet, use C++ Insights. If you want a map of an existing repository, use this project.

## Why it exists

Large C/C++ projects often arrive without a guide: a partial build, years of macros, and no one left who remembers which directory is the entry point. Reading that tree file by file is slow, and sending the whole tree to a hosted model is often unacceptable.

CppInsight AI keeps the scan, the index, the report, and the graphs on your computer. You use them to answer "where do I start?" and "who calls this?" before changing code. An optional model can explain a short cited excerpt after you confirm the file and line range. You can turn that off and keep every local feature.

## What the analysis actually is

The scanner (`src/project/ProjectScanner.cpp`) is a C++17 **heuristic**. It walks the tree and uses regular expressions and line scanning to estimate includes, symbols, calls, type relations, and entry points. It does not build a Clang AST and it does not run libTooling.

`compile_commands.json` is optional and narrow. When it is present, the loader collects include search paths and remembers which files the compilation database lists. The scan mode is then `hybrid`. Symbols and calls in those listed files are stored with the confidence label `precise`.

**`precise` does not mean compiler-checked or AST-accurate.** It means the file appeared in the compilation database and local includes were resolved against those include paths. The string is part of Graph Protocol v1, the MCP schema, saved indexes, and tests, so the code still emits `precise`. In the desktop filter the same value is shown as "编译数据库" (compilation database). Read it as **compile-database-assisted**, then confirm the evidence line in the source.

Without a compilation database the mode and confidence stay `heuristic`. Macros, templates, `#if`, function pointers, and virtual calls can all be missed or mis-linked. Reports and graphs keep that uncertainty on purpose.

## Features

- **Five relationship graphs**, one protocol for the desktop, the Workbench CLI, and MCP:
  - module dependencies
  - function calls
  - class relations
  - file includes
  - entry paths
- **Impact analysis** for a selected function: direct and indirect callers, callees, and a short evidence chain. This is an index-scoped hint, not a proof that a change is safe.
- **Local SQLite index** at `<project>/.cppinsight/index.sqlite`, with incremental reuse and a cancelable rescan that keeps the previous complete index.
- **Optional AI**, including a persistent local-only mode. Local-only mode still scans, reports, graphs, and answers from the local index. It does not call a model.
- **CLI** (`cppinsight`) for scan, overview, and a written brief.
- **MCP server** (`tools/mcp-server`) with `cppinsight_inspect`, `cppinsight_ask`, and `cppinsight_graph` for a local agent.

Search, depth, direction, layout save, and SVG/PNG export live in the desktop graph workbench. The bundled sample (`examples/sample-cpp`) opens from the welcome screen as the built-in sample, so you can try the onboarding report without bringing your own tree.

## Limitations

- Heuristic parsing. Do not treat a graph edge as something the compiler verified.
- A compilation database improves include lookup and changes the stored confidence label. It does not re-parse the file with a compiler.
- No published accuracy numbers yet on large third-party repositories. The in-tree sample only shows that the pipeline runs.
- The packaged app is a Windows desktop (MSI/NSIS). Current candidate installers are unsigned. SmartScreen can warn until Authenticode signing is configured. See [docs/releasing-windows.md](docs/releasing-windows.md).
- Linux and macOS can build the scanner and much of the desktop for development. They are not the release target.
- AI, when you leave it enabled, sends only the confirmed excerpt to the OpenAI-compatible endpoint you configure. Review that range before you continue.

## Quick start

### Scanner (CMake)

Requires CMake 3.16+ and a C++17 compiler.

```bash
cmake -S . -B build -DCMAKE_BUILD_TYPE=Release -DBUILD_TESTING=ON
cmake --build build --config Release
ctest --test-dir build -C Release --output-on-failure
```

Windows multi-config generators put the binary in `build\Release\cppinsight.exe`. Single-config generators (Ninja, Make) put it in `build/cppinsight`.

```bash
./build/cppinsight scan examples/sample-cpp --json scan.json --overview overview.md
./build/cppinsight overview examples/sample-cpp --output overview.md
```

Pass `--compile-commands <file>` when you already have a compilation database. Add `--incremental <previous-json>` to reuse an earlier scan.

### Desktop

The Windows launcher from a source checkout is `启动CppInsight.bat`, or:

```powershell
.\scripts\start-desktop.ps1
```

From source, the desktop app needs Rust (stable), Node.js 22, pnpm 11.19, and a built scanner:

```powershell
cd desktop
pnpm install --frozen-lockfile
pnpm run check
```

`pnpm run check` runs the release-config verifier, the production frontend build, the frontend regression tests, and `cargo test`. Packaging (NSIS and MSI) is `.\scripts\package-app.ps1` from the repository root. Details: [desktop/README.md](desktop/README.md).

After the first index, open the onboarding report and switch graphs from there. Settings can disable AI; the status then shows local-only analysis.

### Workbench CLI and MCP

```powershell
cargo build --release --manifest-path desktop\src-tauri\Cargo.toml --bin cppinsight-workbench
.\desktop\src-tauri\target\release\cppinsight-workbench.exe inspect --root D:\path\to\project encryptFile
```

MCP setup: [tools/mcp-server/README.md](tools/mcp-server/README.md). Protocol: [docs/graph-protocol-v1.md](docs/graph-protocol-v1.md).

## Privacy

Scanning, indexing, graphs, and impact queries stay on the machine. An API key, if you set one, goes to the operating-system credential store (Windows Credential Manager, or Keychain on macOS). It is not written to the project SQLite file and it is not returned to the webview. Diagnostic export is an aggregate preview: version, platform, an anonymous project id, and counts. It is not your source, your paths, your chat, or your key.

`OPENAI_BASE_URL`, `OPENAI_API_KEY`, and `OPENAI_MODEL` override the values saved in the desktop settings.

## Screenshots

Real captures are not in the tree yet. Please add these files before a Show HN / r/cpp / V2EX / 知乎 post. Until then, do not imply that the pictures already exist.

| File | What to capture |
| --- | --- |
| `docs/screenshots/01-welcome.png` | Welcome screen with the built-in sample and open-project actions |
| `docs/screenshots/02-onboarding-report.png` | Onboarding report after indexing `examples/sample-cpp` |
| `docs/screenshots/03-function-calls.png` | Function-call graph with a node and its evidence |
| `docs/screenshots/04-module-dependencies.png` | Module-dependency graph |
| `docs/screenshots/05-class-relations.png` | Class-relationship graph |
| `docs/screenshots/06-file-includes.png` | File-include graph |
| `docs/screenshots/07-entry-paths.png` | Entry-path graph |
| `docs/screenshots/08-impact.png` | Impact panel: callers or callees and the evidence chain |
| `docs/screenshots/09-source-preview.png` | Source preview opened from a graph node or evidence line |
| `docs/screenshots/10-local-only-settings.png` | Settings with AI off, showing local-only mode and the privacy note |

Suggested caption under each image once it exists: the sample project, the graph kind, and a reminder that edges are heuristic or compile-database-assisted.

## Project status

The source tree builds and has automated tests (Windows CI runs the scanner, frontend, Rust, MCP, installer smoke, and a sample graph benchmark). It is not yet a signed public download. Accuracy on real multi-million-line trees, Authenticode, and the screenshots above are still open. Track the release checklist in [docs/public-beta-readiness.md](docs/public-beta-readiness.md).

## License

The scanner, tests, desktop app, CLI, MCP server, examples, and docs are Apache-2.0. See [LICENSE](LICENSE), [NOTICE](NOTICE), [THIRD_PARTY_NOTICES](THIRD_PARTY_NOTICES), and [docs/licensing.md](docs/licensing.md).

## Contributing

Issues and pull requests are welcome. Read [CONTRIBUTING.md](CONTRIBUTING.md) before sending a change. Bug reports: use the GitHub issue form and do not paste customer source, indexes, absolute paths, or API keys. A redacted diagnostic export from Settings is the right attachment.
