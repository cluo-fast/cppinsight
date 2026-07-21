# CppInsight AI Desktop

This is the Tauri + React workbench for CppInsight AI.

## Development

1. Build the C++ CLI from the repository root:

   ```powershell
   cmake -S . -B build-codex
   cmake --build build-codex --config Release
   ```

2. Install desktop dependencies:

   ```powershell
   cd desktop
   pnpm install
   pnpm tauri dev
   ```

3. Optional AI configuration:

   - Set `OPENAI_API_KEY` before launching, or enter a key in the app settings panel.
   - Override the model with `OPENAI_MODEL`; otherwise the backend uses `gpt-4o-mini`.

The Rust backend calls the C++ CLI as a sidecar process and stores indexes in a local SQLite database under the app data directory.
