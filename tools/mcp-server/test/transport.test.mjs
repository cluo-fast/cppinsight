import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import test from "node:test";
import path from "node:path";
import { fileURLToPath } from "node:url";

const testRoot = path.dirname(fileURLToPath(import.meta.url));
const serverPath = path.resolve(testRoot, "..", "index.mjs");

function startServer() {
  const child = spawn(process.execPath, [serverPath], {
    cwd: path.dirname(serverPath),
    stdio: ["pipe", "pipe", "pipe"],
    windowsHide: true
  });
  let buffer = "";
  const pending = [];
  child.stdout.setEncoding("utf8");
  child.stdout.on("data", (chunk) => {
    buffer += chunk;
    while (true) {
      const newline = buffer.indexOf("\n");
      if (newline < 0) break;
      const line = buffer.slice(0, newline).trim();
      buffer = buffer.slice(newline + 1);
      if (!line) continue;
      const waiter = pending.shift();
      if (waiter) waiter.resolve(JSON.parse(line));
    }
  });
  child.on("error", (error) => {
    while (pending.length > 0) pending.shift().reject(error);
  });
  child.on("exit", (code, signal) => {
    if (pending.length > 0) {
      const error = new Error(`MCP server exited before replying (code=${code}, signal=${signal})`);
      while (pending.length > 0) pending.shift().reject(error);
    }
  });
  return {
    child,
    request(message) {
      return new Promise((resolve, reject) => {
        pending.push({ resolve, reject });
        child.stdin.write(`${JSON.stringify(message)}\n`);
      });
    }
  };
}

test("MCP SDK stdio server completes initialize and tools/list", async () => {
  const server = startServer();
  try {
    const initialized = await server.request({
      jsonrpc: "2.0",
      id: 1,
      method: "initialize",
      params: {
        protocolVersion: "2025-06-18",
        capabilities: {},
        clientInfo: { name: "cppinsight-test", version: "0.1.0" }
      }
    });
    assert.equal(initialized.result.serverInfo.name, "cppinsight");
    assert.equal(initialized.result.capabilities.tools !== undefined, true);

    server.child.stdin.write(JSON.stringify({
      jsonrpc: "2.0",
      method: "notifications/initialized",
      params: {}
    }) + "\n");
    const listed = await server.request({
      jsonrpc: "2.0",
      id: 2,
      method: "tools/list",
      params: {}
    });
    const names = listed.result.tools.map((tool) => tool.name);
    assert.deepEqual(names, ["cppinsight_inspect", "cppinsight_ask", "cppinsight_graph"]);
  } finally {
    server.child.kill();
  }
});
