#!/usr/bin/env node
import { Server } from "@modelcontextprotocol/sdk/server/index.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { CallToolRequestSchema, ListToolsRequestSchema } from "@modelcontextprotocol/sdk/types.js";
import { handleToolCall, listToolDefinitions } from "./core.mjs";

export { handleToolCall, listToolDefinitions, runWorkbench } from "./core.mjs";

export async function startServer() {
  const server = new Server(
    { name: "cppinsight", version: "0.1.0" },
    { capabilities: { tools: {} } }
  );
  server.setRequestHandler(ListToolsRequestSchema, async () => ({ tools: listToolDefinitions() }));
  server.setRequestHandler(CallToolRequestSchema, (request) => handleToolCall(request));
  await server.connect(new StdioServerTransport());
}

if (process.argv[1] && process.argv[1].endsWith("index.mjs")) {
  await startServer();
}
