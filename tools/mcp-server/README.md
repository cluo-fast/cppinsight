# CppInsight MCP Server

Stdio MCP bridge for Cursor / other agents. Calls `cppinsight-workbench` (graph inspect + ask).

## Prerequisites

1. Index a project in the desktop app (writes `<root>/.cppinsight/index.sqlite`).
2. Build the workbench binary:

```powershell
cd desktop/src-tauri
cargo build --release --bin cppinsight-workbench
```

3. Install MCP deps:

```powershell
cd tools/mcp-server
npm install
```

## Cursor MCP config

Add to your MCP settings (path adjust as needed):

```json
{
  "mcpServers": {
    "cppinsight": {
      "command": "node",
      "args": ["D:/wpscode/cppinsight/tools/mcp-server/index.mjs"],
      "env": {
        "CPPINSIGHT_WORKBENCH": "D:/wpscode/cppinsight/desktop/src-tauri/target/release/cppinsight-workbench.exe"
      }
    }
  }
}
```

## Tools

| Tool | Purpose |
| --- | --- |
| `cppinsight_inspect` | Symbol defs / callers / callees / entry paths (JSON) |
| `cppinsight_ask` | Natural-language Q&A with citations |

## CLI equivalent

```powershell
.\desktop\src-tauri\target\release\cppinsight-workbench.exe inspect --root D:\path\to\project encryptFile
.\desktop\src-tauri\target\release\cppinsight-workbench.exe ask --root D:\path\to\project "入口在哪？"
```
