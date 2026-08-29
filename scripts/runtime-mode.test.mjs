import assert from "node:assert/strict";
import test from "node:test";

const runtimeModule = await import("../desktop/src/runtime.ts");

test("runtime detection rejects a normal browser window", () => {
  globalThis.window = {};
  assert.equal(runtimeModule.isTauriRuntime(), false);
  delete globalThis.window;
});

test("runtime detection accepts the Tauri IPC bridge", () => {
  globalThis.window = { __TAURI_INTERNALS__: {} };
  assert.equal(runtimeModule.isTauriRuntime(), true);
  delete globalThis.window;
});
