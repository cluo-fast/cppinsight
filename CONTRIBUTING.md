# Contributing to CppInsight AI

[简体中文](#简体中文)

Thank you for helping. The project is Apache-2.0. By submitting a contribution you agree that it is licensed under the same terms (see [LICENSE](LICENSE) and [docs/licensing.md](docs/licensing.md)).

## What belongs in a change

- Bug fixes, tests, docs, and scanner/graph improvements that keep the heuristic boundary honest.
- User-facing text that says the scanner is heuristic and that `compile_commands.json` only assists include paths and confidence labels.
- Do not describe `precise` as compiler-verified or AST analysis. That string is a stored protocol value. Renaming it changes Graph Protocol v1, the MCP schema, saved indexes, and tests. Propose that separately, with a compatibility plan, instead of renaming it in a drive-by patch.

## What does not belong in the repository

- API keys, certificates, signing passwords, customer source, project indexes, or absolute paths from your machine.
- `build/`, `build-codex/`, `build-release/`, and other CMake output. They are gitignored. An earlier commit added some of these by mistake; do not add them back.
- Generated `cppinsight.exe` under `desktop/src-tauri/resources/`.

## Build and test

Scanner, from the repository root (CMake 3.16+, C++17):

```bash
cmake -S . -B build -DCMAKE_BUILD_TYPE=Release -DBUILD_TESTING=ON
cmake --build build --config Release
ctest --test-dir build -C Release --output-on-failure
```

Desktop checks need Rust stable, Node.js 22, and pnpm 11.19:

```powershell
cd desktop
pnpm install --frozen-lockfile
pnpm run check
```

MCP transport tests:

```powershell
cd tools/mcp-server
npm ci
npm test
```

Windows packaging and installer smoke run in `.github/workflows/windows-ci.yml`. A green Linux scanner test does not replace that job.

## Pull requests

- Describe the user-visible behavior and how you tested it.
- Keep protocol changes explicit. Graph request and response fields are documented in [docs/graph-protocol-v1.md](docs/graph-protocol-v1.md).
- Match the surrounding style. Do not reformat unrelated files.
- If you add a direct dependency, update [THIRD_PARTY_NOTICES](THIRD_PARTY_NOTICES) with its locked version and license.

## Reporting issues

Use the bug or feature template. Include the app version and steps. Attach a redacted diagnostic JSON from Settings if it helps. Do not include source, API keys, customer paths, or chat transcripts.

## 简体中文

感谢贡献。本仓库采用 Apache-2.0。提交贡献即表示你同意以同一许可证授权，见 [LICENSE](LICENSE) 与 [docs/licensing.md](docs/licensing.md)。

请保持对外说法诚实：扫描器是启发式的，`compile_commands.json` 只辅助 include 路径和置信度标签。不要把 `precise` 写成编译器或 AST 已经验证。这个字符串是协议里的存储值，改名会影响 Graph Protocol v1、MCP、已有索引和测试。

不要提交 API Key、证书、客户源码、项目索引或本机绝对路径，也不要提交 `build/`、`build-codex/` 等构建输出。

本地验证请先跑上面的 CMake/CTest，桌面改动再跑 `pnpm run check`。Pull Request 里写清用户能看到的行为变化，以及你怎么测的。新增直接依赖时，请同时更新 [THIRD_PARTY_NOTICES](THIRD_PARTY_NOTICES)。
