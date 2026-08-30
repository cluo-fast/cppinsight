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
npm ci
```

安装依赖后运行 `npm test` 会启动真实 SDK stdio 服务，完成 `initialize` 和 `tools/list` 握手，并另外覆盖图查询参数转换与错误传递。

协议转换的无 SDK 契约测试可在受限环境直接运行：

```powershell
node --test test/*.test.mjs
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
| `cppinsight_graph` | Versioned module/call/class/include/entry-path graph query (JSON) |

`cppinsight_graph` accepts `projectPath`, one of the five `graphKind` values, optional roots/direction/depth/filters/path target and `maxNodes` (1–500). It forwards the request through stdin to the Workbench CLI, so paths and filter content are not embedded in one long command argument. Desktop, CLI and MCP all use protocol v1; graph analysis stays local and does not require AI.

## CLI equivalent

```powershell
.\desktop\src-tauri\target\release\cppinsight-workbench.exe inspect --root D:\path\to\project encryptFile
.\desktop\src-tauri\target\release\cppinsight-workbench.exe ask --root D:\path\to\project "入口在哪？"
.\desktop\src-tauri\target\release\cppinsight-workbench.exe graph --root D:\path\to\project --request graph-request.json
.\desktop\src-tauri\target\release\cppinsight-workbench.exe graph --root D:\path\to\project --request - < graph-request.json
```
