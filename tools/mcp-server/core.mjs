import { spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "../..");

function resolveWorkbenchBin() {
  if (process.env.CPPINSIGHT_WORKBENCH) return process.env.CPPINSIGHT_WORKBENCH;
  const exe = process.platform === "win32" ? "cppinsight-workbench.exe" : "cppinsight-workbench";
  return path.join(repoRoot, "desktop", "src-tauri", "target", "release", exe);
}

export function runWorkbench(args, { timeoutMs = 120_000, stdin = null } = {}) {
  const bin = resolveWorkbenchBin();
  return new Promise((resolve, reject) => {
    const child = spawn(bin, args, { windowsHide: true, env: process.env });
    let stdout = "";
    let stderr = "";
    const timer = setTimeout(() => {
      child.kill();
      reject(new Error(`cppinsight-workbench timed out after ${timeoutMs}ms`));
    }, timeoutMs);
    child.stdout.on("data", (chunk) => { stdout += chunk.toString("utf8"); });
    child.stderr.on("data", (chunk) => { stderr += chunk.toString("utf8"); });
    child.on("error", (error) => {
      clearTimeout(timer);
      reject(new Error(`failed to start workbench at ${bin}: ${error.message}. Build with: cargo build --release --bin cppinsight-workbench`));
    });
    child.on("close", (code) => {
      clearTimeout(timer);
      if (code === 0) resolve(stdout);
      else reject(new Error((stderr || stdout || `workbench exited with code ${code}`).trim()));
    });
    if (stdin != null) child.stdin.end(stdin);
  });
}

const GRAPH_KINDS = ["module_dependencies", "function_calls", "class_relations", "file_includes", "entry_paths"];

export function listToolDefinitions() {
  return [
    {
      name: "cppinsight_inspect",
      description: "Inspect a C/C++ symbol in a CppInsight-indexed project: definitions, callers, callees, entry paths. Returns JSON.",
      inputSchema: { type: "object", properties: {
        projectPath: { type: "string", description: "Absolute path to the indexed project root" },
        query: { type: "string", description: "Symbol name or identifier to inspect" },
        depth: { type: "number", description: "Call graph depth (1-4, default 2)" }
      }, required: ["projectPath", "query"] }
    },
    {
      name: "cppinsight_ask",
      description: "Ask a natural-language question about an indexed C/C++ project using graph-backed retrieval + LLM settings.",
      inputSchema: { type: "object", properties: {
        projectPath: { type: "string", description: "Absolute path to the indexed project root" },
        question: { type: "string", description: "Question about the codebase" }
      }, required: ["projectPath", "question"] }
    },
    {
      name: "cppinsight_graph",
      description: "Query a bounded, evidence-backed C/C++ relationship graph from an indexed project. Returns the versioned graph protocol JSON.",
      inputSchema: { type: "object", properties: {
        projectPath: { type: "string", description: "Absolute path to the indexed project root" },
        protocolVersion: { type: "number", enum: [1], default: 1 },
        graphKind: { type: "string", enum: GRAPH_KINDS },
        rootNodeIds: { type: "array", items: { type: "string" }, default: [] },
        direction: { type: "string", enum: ["incoming", "outgoing", "both"], default: "both" },
        depth: { type: "number", minimum: 0, maximum: 8, default: 2 },
        filters: { type: "object", properties: {
          moduleIds: { type: "array", items: { type: "string" }, default: [] },
          filePaths: { type: "array", items: { type: "string" }, default: [] },
          symbolKinds: { type: "array", items: { type: "string" }, default: [] },
          relationKinds: { type: "array", items: { type: "string" }, default: [] },
          confidence: { type: "array", items: { type: "string", enum: ["resolved", "hybrid", "heuristic", "ambiguous", "unresolved"] }, default: [] },
          search: { type: ["string", "null"], default: null }
        } },
        maxNodes: { type: "number", minimum: 1, maximum: 500, default: 500 },
        pathTarget: { type: ["string", "null"], default: null },
        continuation: { type: ["string", "null"], default: null }
      }, required: ["projectPath", "graphKind"] }
    }
  ];
}

function graphProtocolRequest(args) {
  const filters = args.filters || {};
  return {
    protocol_version: args.protocolVersion ?? 1,
    project_id: 0,
    graph_kind: args.graphKind,
    root_node_ids: args.rootNodeIds ?? [],
    direction: args.direction ?? "both",
    depth: args.depth ?? 2,
    filters: {
      module_ids: filters.moduleIds ?? [],
      file_paths: filters.filePaths ?? [],
      symbol_kinds: filters.symbolKinds ?? [],
      relation_kinds: filters.relationKinds ?? [],
      confidence: filters.confidence ?? [],
      search: filters.search ?? null
    },
    max_nodes: args.maxNodes ?? 500,
    path_target: args.pathTarget ?? null,
    continuation: args.continuation ?? null
  };
}

export async function handleToolCall(request, runner = runWorkbench) {
  const name = request.params.name;
  const args = request.params.arguments || {};
  try {
    if (name === "cppinsight_inspect") {
      const projectPath = String(args.projectPath || "");
      const query = String(args.query || "");
      if (!projectPath || !query) throw new Error("projectPath and query are required");
      const depth = args.depth == null ? null : String(Math.max(1, Math.min(4, Number(args.depth) || 2)));
      const cliArgs = ["inspect", "--root", projectPath];
      if (depth) cliArgs.push("--depth", depth);
      cliArgs.push(query);
      return { content: [{ type: "text", text: (await runner(cliArgs)).trim() }] };
    }
    if (name === "cppinsight_ask") {
      const projectPath = String(args.projectPath || "");
      const question = String(args.question || "");
      if (!projectPath || !question) throw new Error("projectPath and question are required");
      return { content: [{ type: "text", text: (await runner(["ask", "--root", projectPath, question], { timeoutMs: 180_000 })).trim() }] };
    }
    if (name === "cppinsight_graph") {
      const projectPath = String(args.projectPath || "");
      if (!projectPath || !args.graphKind) throw new Error("projectPath and graphKind are required");
      return { content: [{ type: "text", text: (await runner(["graph", "--root", projectPath, "--request", "-"], { stdin: `${JSON.stringify(graphProtocolRequest(args))}\n` })).trim() }] };
    }
    throw new Error(`Unknown tool: ${name}`);
  } catch (error) {
    return { isError: true, content: [{ type: "text", text: error instanceof Error ? error.message : String(error) }] };
  }
}
