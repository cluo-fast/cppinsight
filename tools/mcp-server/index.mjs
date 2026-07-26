#!/usr/bin/env node
import { spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { Server } from "@modelcontextprotocol/sdk/server/index.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import {
  CallToolRequestSchema,
  ListToolsRequestSchema
} from "@modelcontextprotocol/sdk/types.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "../..");

function resolveWorkbenchBin() {
  if (process.env.CPPINSIGHT_WORKBENCH) {
    return process.env.CPPINSIGHT_WORKBENCH;
  }
  const exe =
    process.platform === "win32" ? "cppinsight-workbench.exe" : "cppinsight-workbench";
  return path.join(
    repoRoot,
    "desktop",
    "src-tauri",
    "target",
    "release",
    exe
  );
}

function runWorkbench(args, { timeoutMs = 120_000 } = {}) {
  const bin = resolveWorkbenchBin();
  return new Promise((resolve, reject) => {
    const child = spawn(bin, args, {
      windowsHide: true,
      env: process.env
    });
    let stdout = "";
    let stderr = "";
    const timer = setTimeout(() => {
      child.kill();
      reject(new Error(`cppinsight-workbench timed out after ${timeoutMs}ms`));
    }, timeoutMs);

    child.stdout.on("data", (chunk) => {
      stdout += chunk.toString("utf8");
    });
    child.stderr.on("data", (chunk) => {
      stderr += chunk.toString("utf8");
    });
    child.on("error", (error) => {
      clearTimeout(timer);
      reject(
        new Error(
          `failed to start workbench at ${bin}: ${error.message}. Build with: cargo build --release --bin cppinsight-workbench`
        )
      );
    });
    child.on("close", (code) => {
      clearTimeout(timer);
      if (code === 0) {
        resolve(stdout);
        return;
      }
      reject(
        new Error(
          (stderr || stdout || `workbench exited with code ${code}`).trim()
        )
      );
    });
  });
}

const server = new Server(
  { name: "cppinsight", version: "0.1.0" },
  { capabilities: { tools: {} } }
);

server.setRequestHandler(ListToolsRequestSchema, async () => ({
  tools: [
    {
      name: "cppinsight_inspect",
      description:
        "Inspect a C/C++ symbol in a CppInsight-indexed project: definitions, callers, callees, entry paths. Returns JSON.",
      inputSchema: {
        type: "object",
        properties: {
          projectPath: {
            type: "string",
            description: "Absolute path to the indexed project root"
          },
          query: {
            type: "string",
            description: "Symbol name or identifier to inspect"
          },
          depth: {
            type: "number",
            description: "Call graph depth (1-4, default 2)"
          }
        },
        required: ["projectPath", "query"]
      }
    },
    {
      name: "cppinsight_ask",
      description:
        "Ask a natural-language question about an indexed C/C++ project using graph-backed retrieval + LLM settings.",
      inputSchema: {
        type: "object",
        properties: {
          projectPath: {
            type: "string",
            description: "Absolute path to the indexed project root"
          },
          question: {
            type: "string",
            description: "Question about the codebase"
          }
        },
        required: ["projectPath", "question"]
      }
    }
  ]
}));

server.setRequestHandler(CallToolRequestSchema, async (request) => {
  const name = request.params.name;
  const args = request.params.arguments || {};

  try {
    if (name === "cppinsight_inspect") {
      const projectPath = String(args.projectPath || "");
      const query = String(args.query || "");
      if (!projectPath || !query) {
        throw new Error("projectPath and query are required");
      }
      const depth =
        args.depth == null ? null : String(Math.max(1, Math.min(4, Number(args.depth) || 2)));
      const cliArgs = ["inspect", "--root", projectPath];
      if (depth) {
        cliArgs.push("--depth", depth);
      }
      cliArgs.push(query);
      const stdout = await runWorkbench(cliArgs);
      return {
        content: [{ type: "text", text: stdout.trim() }]
      };
    }

    if (name === "cppinsight_ask") {
      const projectPath = String(args.projectPath || "");
      const question = String(args.question || "");
      if (!projectPath || !question) {
        throw new Error("projectPath and question are required");
      }
      const stdout = await runWorkbench(
        ["ask", "--root", projectPath, question],
        { timeoutMs: 180_000 }
      );
      return {
        content: [{ type: "text", text: stdout.trim() }]
      };
    }

    throw new Error(`Unknown tool: ${name}`);
  } catch (error) {
    return {
      isError: true,
      content: [
        {
          type: "text",
          text: error instanceof Error ? error.message : String(error)
        }
      ]
    };
  }
});

const transport = new StdioServerTransport();
await server.connect(transport);
