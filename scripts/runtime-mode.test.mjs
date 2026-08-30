import assert from "node:assert/strict";
import test from "node:test";

const runtimeModule = await import("../desktop/src/runtime.ts");

test("runtime detection rejects a normal browser window", () => {
  globalThis.window = {};
  assert.equal(runtimeModule.isTauriRuntime(), false);
  assert.equal(
    runtimeModule.desktopActionMessage(),
    "浏览器预览模式：请在 CppInsight AI 桌面应用中打开项目。"
  );
  assert.equal(runtimeModule.isDesktopActionDisabled(), true);
  delete globalThis.window;
});

test("runtime detection accepts the Tauri IPC bridge", () => {
  globalThis.window = { __TAURI_INTERNALS__: {} };
  assert.equal(runtimeModule.isTauriRuntime(), true);
  assert.equal(runtimeModule.desktopActionMessage(), null);
  assert.equal(runtimeModule.isDesktopActionDisabled(), false);
  delete globalThis.window;
});
